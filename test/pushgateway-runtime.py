"""Check a supplied Pushgateway binary with synthetic metrics and test credentials.

Requires Python 3.9+, PyYAML, Helm, and the upstream chart. No Kubernetes
cluster or production credentials are used. All files stay in --output-dir.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
import yaml

parser = argparse.ArgumentParser(description=__doc__)
for option in ['binary', 'helm', 'chart', 'output-dir']:
    parser.add_argument('--' + option, type=Path, required=True)
args = parser.parse_args()
args.output_dir.mkdir(parents=True, exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='pushgateway-native-', dir=args.output_dir.resolve()))
password = 'catalog-native-validation-only'
rendered = subprocess.run([str(args.helm.resolve()), 'template', 'fixture', str(args.chart.resolve()),
    '--set-string', 'webConfiguration.basicAuthUsers.batch=' + password],
    check=True, capture_output=True, encoding='utf-8', timeout=30).stdout
secrets = [obj for obj in yaml.safe_load_all(rendered) if obj and obj.get('kind') == 'Secret']
assert len(secrets) == 1, 'Expected the chart to generate one test web configuration Secret'
web_config = work / 'web-config.yaml'
web_config.write_bytes(base64.b64decode(secrets[0]['data']['web-config.yaml']))
assert yaml.safe_load(web_config.read_text(encoding='utf-8'))['basic_auth_users']['batch']
with socket.socket() as s:
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
base = 'http://127.0.0.1:' + str(port)
database = work / 'pushgateway.data'
# v1.11.3 rejects a Windows backslash absolute persistence path when creating
# its temporary snapshot file. The explicit cwd keeps this relative file in
# the isolated test directory; the Kubernetes /data path is checked separately.
command = [str(args.binary.resolve()), '--web.listen-address=127.0.0.1:' + str(port),
           '--web.config.file=' + str(web_config), '--persistence.file=' + database.name,
           '--persistence.interval=5s']
checks = []
process = None
log = None
launches = 0

def request(method, path, payload=None, credential=password):
    headers = {}
    if credential is not None:
        headers['Authorization'] = 'Basic ' + base64.b64encode(('batch:' + credential).encode()).decode()
    if payload is not None:
        headers['Content-Type'] = 'text/plain; version=0.0.4'
        payload = payload.encode('utf-8')
    req = Request(base + path, data=payload, method=method, headers=headers)
    try:
        with urlopen(req, timeout=5) as response:
            return response.status, response.read().decode('utf-8')
    except HTTPError as response:
        return response.code, response.read().decode('utf-8')

def check(name, condition):
    if not condition:
        raise AssertionError(name)
    checks.append(name)

def start():
    global process, log, launches
    launches += 1
    log = (work / ('server-' + str(launches) + '.log')).open('wb')
    process = subprocess.Popen(command, cwd=work, stdout=log, stderr=subprocess.STDOUT,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError('Pushgateway exited; inspect ' + str(work))
        try:
            if request('GET', '/-/ready')[0] == 200:
                return
        except (URLError, TimeoutError, ConnectionError):
            pass
        time.sleep(0.1)
    raise TimeoutError('Pushgateway did not become HTTP ready')

def stop():
    global process, log
    if process is not None:
        process.terminate()
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        process = None
    if log is not None:
        log.close()
        log = None

def sample(name, job):
    status, body = request('GET', '/metrics')
    assert status == 200
    found = []
    for line in body.splitlines():
        if line.startswith(name + '{'):
            labels = dict(re.findall(r'(\w+)="((?:\\.|[^"\\])*)"', line))
            if labels.get('job') == job:
                found.append((float(line.rsplit(' ', 1)[1]), labels))
    assert len(found) <= 1
    return found[0] if found else None

def snapshot_after(previous):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if database.exists() and database.stat().st_size and database.stat().st_mtime_ns > previous:
            return
        time.sleep(0.1)
    raise TimeoutError('No periodic persistence snapshot was written')

job = '/metrics/job/catalog_probe'
try:
    start()
    for path in ['/metrics', '/-/healthy', '/-/ready']:
        check('anonymous GET ' + path + ' rejected', request('GET', path, credential=None)[0] == 401)
    check('wrong password rejected', request('GET', '/metrics', credential='wrong')[0] == 401)
    check('authenticated readiness', request('GET', '/-/ready')[0] == 200)
    check('authenticated health', request('GET', '/-/healthy')[0] == 200)
    for method, data in [('PUT', 'catalog_probe_success 999\n'), ('POST', 'catalog_extra 999\n'), ('DELETE', None)]:
        check('anonymous ' + method + ' rejected', request(method, job, data, None)[0] == 401)
    previous = database.stat().st_mtime_ns if database.exists() else 0
    check('push accepted', request('PUT', job, 'catalog_probe_success 1\n')[0] == 200)
    check('job labels preserved', sample('catalog_probe_success', 'catalog_probe') ==
          (1.0, {'instance': '', 'job': 'catalog_probe'}))
    check('wrong-password update rejected', request('PUT', job, 'catalog_probe_success 999\n', 'wrong')[0] == 401)
    check('rejected update preserves value', sample('catalog_probe_success', 'catalog_probe')[0] == 1)
    check('malformed metrics rejected', request('PUT', job, 'not valid metric text !\n')[0] == 400)
    check('malformed update preserves value', sample('catalog_probe_success', 'catalog_probe')[0] == 1)
    check('POST merges a second metric', request('POST', job, 'catalog_extra 7\n')[0] == 200)
    check('POST preserves original metric', sample('catalog_probe_success', 'catalog_probe')[0] == 1)
    check('POST stores second metric', sample('catalog_extra', 'catalog_probe')[0] == 7)
    check('independent group accepted', request('PUT', '/metrics/job/control_group', 'catalog_control 99\n')[0] == 200)
    snapshot_after(previous)
    stop()
    start()
    check('authentication remains after restart', request('GET', '/metrics', credential=None)[0] == 401)
    check('original metric persisted', sample('catalog_probe_success', 'catalog_probe')[0] == 1)
    check('merged metric persisted', sample('catalog_extra', 'catalog_probe')[0] == 7)
    check('control group persisted', sample('catalog_control', 'control_group')[0] == 99)
    check('anonymous DELETE rejected', request('DELETE', job, credential=None)[0] == 401)
    check('unauthorized DELETE preserves group', sample('catalog_extra', 'catalog_probe')[0] == 7)
    previous = database.stat().st_mtime_ns
    check('authenticated DELETE queued', request('DELETE', job)[0] == 202)
    deadline = time.monotonic() + 3
    while sample('catalog_probe_success', 'catalog_probe') is not None and time.monotonic() < deadline:
        time.sleep(0.1)
    check('selected group removed', sample('catalog_probe_success', 'catalog_probe') is None and
          sample('catalog_extra', 'catalog_probe') is None)
    check('other group unaffected', sample('catalog_control', 'control_group')[0] == 99)
    snapshot_after(previous)
    stop()
    start()
    check('deletion persisted', sample('catalog_probe_success', 'catalog_probe') is None and
          sample('catalog_extra', 'catalog_probe') is None)
    check('other group survives second restart', sample('catalog_control', 'control_group')[0] == 99)
finally:
    stop()

version = subprocess.run([str(args.binary.resolve()), '--version'], capture_output=True,
                         text=True, encoding='utf-8', check=True, timeout=5).stdout.splitlines()[0]
result = {'passed': True, 'version': version, 'checks': checks, 'check_count': len(checks),
          'launches': launches, 'platform': os.name, 'persistence_interval': '5s',
          'persistence_path': 'relative filename inside the isolated test working directory',
          'shutdown': 'TerminateProcess on Windows; test waits for periodic snapshots before restart',
          'live_kubernetes_tested': False, 'fixture_directory': work.name}
(work / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result, indent=2))
