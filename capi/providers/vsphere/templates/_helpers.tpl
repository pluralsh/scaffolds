{{/*
Expand the name of the chart.
*/}}
{{- define "cluster-api-provider-vsphere.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "cluster-api-provider-vsphere.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- $releaseName := replace .Chart.Name "capv" .Release.Name }}
{{- if contains $name $releaseName }}
{{- $releaseName | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" $releaseName $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "cluster-api-provider-vsphere.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "cluster-api-provider-vsphere.labels" -}}
helm.sh/chart: {{ include "cluster-api-provider-vsphere.chart" . }}
{{ include "cluster-api-provider-vsphere.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "cluster-api-provider-vsphere.selectorLabels" -}}
app.kubernetes.io/name: {{ include "cluster-api-provider-vsphere.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
CAPV controller namespace
*/}}
{{- define "cluster-api-provider-vsphere.namespace" -}}
{{- .Values.operator.namespace | default "capv-system" }}
{{- end }}

{{/*
Credentials secret name
*/}}
{{- define "cluster-api-provider-vsphere.credentialsSecretName" -}}
{{- printf "%s-credentials" (include "cluster-api-provider-vsphere.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}
