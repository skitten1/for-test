{{- define "dognauts-kmcp.projectAnchorLinks" -}}
{{- $links := .Values.projectAnchor.nestedChart.links | default dict -}}
dognauts-kmcp:
  {{- toYaml $links | nindent 2 }}
  slug: {{ printf "kmcp-%s" .Release.Name | quote }}
  labels:
    en: &label_en {{ .Release.Name | quote }}
    ru: *label_en
  link: ""
  nestedReleaseName: {{ .Release.Name | quote }}
  chartName: {{ .Chart.Name | quote }}
  chartVersion: {{ .Chart.Version | quote }}
{{- end -}}