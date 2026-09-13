"""Exercise the catalog profile with a verified Tika 4 distribution and Java 17+.

Uses synthetic documents, loopback HTTP and isolated temporary storage. Does not
download anything or test Kubernetes, the container filesystem, CNI or OCR.
"""
import argparse
import io
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import ProxyHandler, Request, build_opener
import zipfile


def docx_fixture():
    stream = io.BytesIO()
    entries = {
        '[Content_Types].xml': '''<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>''',
        '_rels/.rels': '''<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>''',
        'word/document.xml': '''<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>Catalog DOCX fixture. 林间文档。</w:t></w:r></w:p><w:sectPr/></w:body>
</w:document>''',
    }
    with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, text in entries.items():
            archive.writestr(name, text.encode('utf-8'))
    return stream.getvalue()


def pdf_fixture():
    # A single synthetic page with an actual text layer, no external generator.
    content = b'BT /F1 12 Tf 72 720 Td (Catalog PDF text-layer fixture.) Tj ET\n'
    objects = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
        b'<< /Length ' + str(len(content)).encode() + b' >>\nstream\n' + content + b'endstream',
    ]
    data = bytearray(b'%PDF-1.4\n')
    offsets = [0]
    for index, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(str(index).encode() + b' 0 obj\n' + obj + b'\nendobj\n')
    xref = len(data)
    data.extend(b'xref\n0 6\n0000000000 65535 f \n')
    for offset in offsets[1:]:
        data.extend(f'{offset:010d} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    return bytes(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--java', type=Path, required=True)
    parser.add_argument('--tika-home', type=Path, required=True,
                        help='Extracted full distribution containing lib/ and plugins/')
    parser.add_argument('--config', type=Path,
                        default=Path(__file__).resolve().parents[1] /
                        'catalogs/data/tika/kubernetes/tika-config.json')
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    java = args.java.resolve()
    tika_home = args.tika_home.resolve()
    if not java.is_file() or not (tika_home / 'lib/tika-pipes-core-4.0.0.jar').is_file():
        raise ValueError('Provide Java and the complete, verified Tika 4.0.0 distribution')
    args.output_dir.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='tika-native-', dir=args.output_dir.resolve()))
    temporary = work / 'tmp'
    temporary.mkdir()
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    config = json.loads(args.config.read_text(encoding='utf-8'))
    # Preserve all production gates and limits. Only adapt bind and local paths.
    config['server']['host'] = '127.0.0.1'
    config['server']['port'] = port
    config['pipes']['javaPath'] = str(java)
    config['pipes']['tempDirectory'] = str(temporary)
    config_file = work / 'tika-config.json'
    config_file.write_text(json.dumps(config, indent=2) + '\n', encoding='utf-8')
    env = os.environ.copy()
    env.pop('_JAVA_OPTIONS', None)
    env.pop('JDK_JAVA_OPTIONS', None)
    env['JAVA_TOOL_OPTIONS'] = ('-Xms64m -Xmx256m -Dfile.encoding=UTF-8 '
                               '-Djava.io.tmpdir="' + temporary.as_posix() +
                               '" -Duser.home="' + work.as_posix() + '"')
    env['TEMP'] = env['TMP'] = str(temporary)
    env['HOME'] = str(work)
    env['XDG_CACHE_HOME'] = str(temporary / '.cache')
    # ASCII relative paths avoid Windows launcher @argfile encoding loss in
    # non-ASCII workspaces. The fork inherits the distribution working directory.
    command = [str(java), '-cp', '*' + os.pathsep + 'lib/*',
               'org.apache.tika.server.core.TikaServerCli', '-c', str(config_file),
               '-h', '127.0.0.1', '-p', str(port)]
    base = 'http://127.0.0.1:' + str(port)
    opener = build_opener(ProxyHandler({}))
    checks = []

    def check(condition, label):
        if not condition:
            raise AssertionError(label)
        checks.append(label)

    def request(method, path, body=None, filename='fixture.txt',
                content_type='text/plain; charset=UTF-8', timeout=75):
        headers = {'Content-Type': content_type,
                   'Content-Disposition': 'attachment; filename="' + filename + '"'}
        req = Request(base + path, method=method, data=body, headers=headers)
        try:
            response = opener.open(req, timeout=timeout)
        except HTTPError as error:
            response = error
        with response:
            return response.status, response.read().decode('utf-8', errors='replace')

    result = {'fixture_directory': work.name, 'platform': os.name, 'checks': checks,
              'scope': 'Native HTTP profile only; no Kubernetes/CNI/Linux container, OCR, timeout/OOM or graceful-shutdown tests.'}
    log = (work / 'server.log').open('wb')
    proc = subprocess.Popen(command, cwd=tika_home, env=env,
                            stdout=log, stderr=subprocess.STDOUT,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0,
                            start_new_session=os.name != 'nt')
    try:
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                raise RuntimeError('Tika exited; inspect server.log')
            try:
                status, body = request('GET', '/version', timeout=5)
                if status == 200:
                    break
            except (URLError, TimeoutError, ConnectionError):
                pass
            time.sleep(0.25)
        else:
            raise TimeoutError('Tika did not become HTTP ready')
        check(body == 'Apache Tika 4.0.0', 'Pinned server version responds')

        fixture = 'Catalog UTF-8 fixture. 林间小路。\n'
        status, body = request('PUT', '/tika/text', fixture.encode('utf-8'))
        check(status == 200 and body.strip() == fixture.strip(), 'Plain text preserves Unicode')
        status, body = request('PUT', '/tika/json/text', fixture.encode('utf-8'))
        metadata = json.loads(body)
        check(status == 200 and fixture.strip() in metadata['tk:content'], 'JSON extraction includes text')
        check(metadata['Content-Type'].startswith('text/plain'), 'JSON extraction includes media type')
        check(metadata['tk:resource-name'] == 'fixture.txt', 'Filename metadata survives extraction')
        status, body = request('PUT', '/rmeta/text', fixture.encode('utf-8'))
        recursive = json.loads(body)
        check(status == 200 and len(recursive) == 1 and fixture.strip() in recursive[0]['tk:content'],
              'Recursive metadata returns the text document')
        status, body = request('PUT', '/meta', fixture.encode('utf-8'))
        metadata = json.loads(body)
        check(status == 200 and 'tk:content' not in metadata and 'Content-Type' in metadata,
              'Metadata-only response omits extracted text')
        status, body = request('PUT', '/detect', fixture.encode('utf-8'))
        check(status == 200 and body.strip() == 'text/plain', 'Text detection succeeds')

        html = '<!doctype html><html><head><meta charset="utf-8"><title>Catalog HTML title</title></head><body><p>HTML fixture. 林间网页。</p></body></html>'
        status, body = request('PUT', '/tika/json/text', html.encode('utf-8'), 'fixture.html', 'text/html')
        metadata = json.loads(body)
        check(status == 200 and 'HTML fixture. 林间网页。' in metadata['tk:content'], 'HTML text preserves Unicode')
        check(metadata.get('dc:title') == 'Catalog HTML title', 'HTML title is extracted')

        for name, data, mime, expected in [
            ('fixture.docx', docx_fixture(), 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
             'Catalog DOCX fixture. 林间文档。'),
            ('fixture.pdf', pdf_fixture(), 'application/pdf', 'Catalog PDF text-layer fixture.'),
        ]:
            (work / name).write_bytes(data)
            status, body = request('PUT', '/tika/json/text', data, name, 'application/octet-stream')
            metadata = json.loads(body)
            check(status == 200 and expected in metadata['tk:content'], name + ' content is parsed')
            check(metadata['Content-Type'].startswith(mime), name + ' media type is detected')
            check(not any(key.startswith('tk:exception:') for key in metadata), name + ' has no parse exception')

        status, _ = request('PUT', '/tika/json/not-a-handler', fixture.encode('utf-8'))
        check(status == 400, 'Unknown handler is rejected with 400')
        for method, path in [('GET', '/status'), ('POST', '/pipes'),
                             ('POST', '/async'), ('PUT', '/unpack')]:
            status, _ = request(method, path, b'{}')
            check(status == 404, 'Disabled endpoint ' + path + ' returns 404')
        boundary = 'tika-catalog-fixture-boundary'
        multipart = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="fixture.txt"\r\n'
                     'Content-Type: text/plain\r\n\r\nSynthetic fixture\r\n'
                     f'--{boundary}\r\nContent-Disposition: form-data; name="config"\r\n'
                     'Content-Type: application/json\r\n\r\n{}\r\n'
                     f'--{boundary}--\r\n').encode('utf-8')
        status, body = request('POST', '/tika/config/text', multipart,
                               content_type='multipart/form-data; boundary=' + boundary)
        check(status == 403,
              f'Per-request parser configuration is rejected with 403 (got {status}: {body[:160]})')

        spooled = '林间。' * 140000 + 'SPOOL_END'
        check(len(spooled.encode('utf-8')) > config['pipes']['maxInlineBytes'], 'Spool fixture exceeds inline threshold')
        status, body = request('PUT', '/tika/text', spooled.encode('utf-8'))
        check(status == 200 and body.strip() == spooled, 'Spooled Unicode document is complete')
        over_output = b'A' * (config['parse-context']['output-limits']['writeLimit'] + 100)
        status, body = request('PUT', '/tika/text', over_output)
        check(status == 422, 'Output limit is reported as a partial parse')
        check(0 < len(body) <= config['parse-context']['output-limits']['writeLimit'],
              'Partial text remains within configured output limit')
        status, body = request('PUT', '/tika/json/text', over_output)
        metadata = json.loads(body)
        check(status == 200 and any(key.startswith('tk:exception:') for key in metadata),
              'JSON output-limit result includes exception metadata')
        # An actual bounded 10 MiB + 1 body tests the upload cap, without relying
        # on a misleading oversized Content-Length or constructing a huge file.
        over_request = b'B' * (config['server']['maxRequestSizeBytes'] + 1)
        status, _ = request('PUT', '/tika/text', over_request)
        check(status == 413, 'Actual oversized request body is rejected with 413')
        status, body = request('PUT', '/tika/text', fixture.encode('utf-8'))
        check(status == 200 and body.strip() == fixture.strip(), 'Parser handles a valid document after rejected inputs')
        result['passed'] = True
    except Exception as error:
        result['passed'] = False
        result['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        if proc.poll() is None:
            if os.name == 'nt':
                subprocess.run(['taskkill', '/PID', str(proc.pid), '/T', '/F'],
                               stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT, timeout=15)
            else:
                os.killpg(proc.pid, signal.SIGTERM)
            proc.wait(timeout=15)
        log.close()
        result['checks_passed'] = len(checks)
        (work / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
