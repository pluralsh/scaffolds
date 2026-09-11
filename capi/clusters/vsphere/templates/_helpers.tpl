{{/*
Expand the name of the chart.
*/}}
{{- define "workload-cluster.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "workload-cluster.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "workload-cluster.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "workload-cluster.labels" -}}
helm.sh/chart: {{ include "workload-cluster.chart" . }}
{{ include "workload-cluster.selectorLabels" . }}
cluster.x-k8s.io/cluster-name: {{ .Values.cluster.name }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "workload-cluster.selectorLabels" -}}
app.kubernetes.io/name: {{ include "workload-cluster.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Cluster-scoped vSphere settings shared by infrastructure resources.
*/}}
{{- define "workload-cluster.vsphere" -}}
{{- $v := .Values.cluster.vsphere -}}
server: {{ $v.server | quote }}
{{- if $v.thumbprint }}
thumbprint: {{ $v.thumbprint | quote }}
{{- end }}
datacenter: {{ $v.datacenter | quote }}
datastore: {{ $v.datastore | quote }}
{{- if $v.folder }}
folder: {{ $v.folder | quote }}
{{- end }}
resourcePool: {{ $v.resourcePool | quote }}
{{- end }}

{{/*
Network devices for VSphereMachine spec.
*/}}
{{- define "workload-cluster.vsphere.networkDevices" -}}
{{- $network := .network | default .ctx.Values.cluster.vsphere.network -}}
{{- $devices := .devices | default (list (dict "dhcp4" true "dhcp6" false "networkName" $network)) -}}
network:
  devices:
  {{- toYaml $devices | nindent 2 }}
{{- end }}

{{/*
SSH authorized keys for kubeadm users block.
*/}}
{{- define "workload-cluster.kubeadmUsers" -}}
{{- $keys := .keys -}}
{{- if $keys }}
users:
- name: capv
  sudo: ALL=(ALL) NOPASSWD:ALL
  sshAuthorizedKeys:
  {{- range $keys }}
  - {{ . | quote }}
  {{- end }}
{{- end }}
{{- end }}

{{/*
Pre-kubeadm commands shared by control plane and workers.
*/}}
{{- define "workload-cluster.preKubeadmCommands" -}}
preKubeadmCommands:
- hostnamectl set-hostname "{{`{{ ds.meta_data.hostname }}`}}"
- echo "::1         ipv6-localhost ipv6-loopback localhost6 localhost6.localdomain6" >/etc/hosts
- echo "127.0.0.1   {{`{{ ds.meta_data.hostname }}`}} {{`{{ local_hostname }}`}} localhost localhost.localdomain localhost4 localhost4.localdomain4" >>/etc/hosts
{{- if .kubeVipEnabled }}
- mkdir -p /etc/pre-kubeadm-commands
- for script in $(find /etc/pre-kubeadm-commands/ -name '*.sh' -type f | sort); do echo "Running script $script"; "$script"; done
{{- end }}
{{- end }}

{{/*
VSphereMachineTemplate for control plane or workers.
  ctx, name, spec (merged machine spec), suffix (optional template name suffix)
*/}}
{{- define "workers.vsphereMachineTemplate" -}}
{{- $v := .ctx.Values.cluster.vsphere -}}
{{- $spec := .spec -}}
apiVersion: infrastructure.cluster.x-k8s.io/v1beta2
kind: VSphereMachineTemplate
metadata:
  name: {{ .ctx.Values.cluster.name }}{{ if .suffix }}-{{ .suffix }}{{ end }}
  labels:
    {{- include "workload-cluster.labels" .ctx | nindent 4 }}
  annotations:
    helm.sh/resource-policy: keep
spec:
  template:
    spec:
      cloneMode: {{ $spec.cloneMode | default $v.cloneMode | default "fullClone" }}
      datacenter: {{ $v.datacenter }}
      datastore: {{ $v.datastore }}
      {{- if $v.folder }}
      folder: {{ $v.folder }}
      {{- end }}
      diskGiB: {{ $spec.diskGiB }}
      memoryMiB: {{ $spec.memoryMiB }}
      {{- $devices := "" -}}
      {{- if and $spec.network $spec.network.devices }}{{- $devices = $spec.network.devices }}{{- end }}
      {{- include "workload-cluster.vsphere.networkDevices" (dict "ctx" .ctx "devices" $devices "network" $v.network) | nindent 6 }}
      numCPUs: {{ $spec.numCPUs }}
      os: {{ $spec.os | default $v.os | default "Linux" }}
      powerOffMode: {{ $spec.powerOffMode | default $v.powerOffMode | default "trySoft" }}
      resourcePool: {{ $v.resourcePool | quote }}
      server: {{ $v.server }}
      template: {{ $v.template }}
{{- end }}

{{/*
KubeadmConfigTemplate for a worker group.
*/}}
{{- define "workers.kubeadmConfigTemplate" -}}
{{- $keys := .keys | default .ctx.Values.workers.defaults.sshAuthorizedKeys -}}
apiVersion: bootstrap.cluster.x-k8s.io/v1beta2
kind: KubeadmConfigTemplate
metadata:
  name: {{ .ctx.Values.cluster.name }}-{{ .name }}
  labels:
    {{- include "workload-cluster.labels" .ctx | nindent 4 }}
  annotations:
    helm.sh/resource-policy: keep
spec:
  template:
    spec:
      joinConfiguration:
        nodeRegistration:
          criSocket: /var/run/containerd/containerd.sock
          kubeletExtraArgs:
          - name: cloud-provider
            value: external
          name: '{{`{{ local_hostname }}`}}'
      {{- include "workload-cluster.preKubeadmCommands" (dict "kubeVipEnabled" false) | nindent 6 }}
      {{- include "workload-cluster.kubeadmUsers" (dict "keys" $keys) | nindent 6 }}
{{- end }}

{{/*
MachineDeployment for a worker group.
*/}}
{{- define "workers.machineDeployment" -}}
{{- $replicas := (.values | default dict).replicas | default .defaultVals.replicas -}}
apiVersion: cluster.x-k8s.io/v1beta2
kind: MachineDeployment
metadata:
  name: {{ .ctx.Values.cluster.name }}-{{ .name }}
  labels:
    {{- include "workload-cluster.labels" .ctx | nindent 4 }}
  annotations:
    helm.sh/resource-policy: keep
    {{- with (.values | default dict).annotations | default .defaultVals.annotations }}
    {{- toYaml . | nindent 4 }}
    {{- end }}
spec:
  clusterName: {{ .ctx.Values.cluster.name }}
  replicas: {{ $replicas }}
  selector:
    matchLabels: {}
  template:
    metadata:
      labels:
        cluster.x-k8s.io/cluster-name: {{ .ctx.Values.cluster.name }}
        {{- with (.values | default dict).labels | default .defaultVals.labels }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
    spec:
      clusterName: {{ .ctx.Values.cluster.name }}
      version: v{{ trimPrefix "v" (.ctx.Values.cluster.kubernetesVersion | toString) }}
      bootstrap:
        configRef:
          apiGroup: bootstrap.cluster.x-k8s.io
          kind: KubeadmConfigTemplate
          name: {{ .ctx.Values.cluster.name }}-{{ .name }}
      infrastructureRef:
        apiGroup: infrastructure.cluster.x-k8s.io
        kind: VSphereMachineTemplate
        name: {{ .ctx.Values.cluster.name }}-{{ .name }}
{{- end }}
