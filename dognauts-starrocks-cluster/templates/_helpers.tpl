{{/*
Общие метки ресурсов чарта
*/}}
{{- define "dognauts-starrocks.labels" -}}
{{ include "dognauts-common-chart.labels" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/part-of: dognauts-starrocks
{{- end }}

{{/*
Имя StarRocksCluster — берём из values upstream-чарта, чтобы не расходиться с ним
*/}}
{{- define "dognauts-starrocks.clusterName" -}}
{{- .Values.starrocks.starrocksCluster.name }}
{{- end }}

{{/*
Сервис FE, который создаёт оператор: <кластер>-fe-service (http 8030, query 9030)
*/}}
{{- define "dognauts-starrocks.feService" -}}
{{- printf "%s-fe-service" (include "dognauts-starrocks.clusterName" .) }}
{{- end }}

{{/*
Метки подов. app.kubernetes.io/component (fe/be/cn) ставит оператор,
name/instance — мы через starrocks.starrocksCluster.componentValues.podLabels
*/}}
{{- define "dognauts-starrocks.selectorLabels" -}}
app.kubernetes.io/name: starrocks
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "dognauts-starrocks.consoleSelectorLabels" -}}
app.kubernetes.io/name: starrocks-console
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "dognauts-starrocks.feSelectorLabels" -}}
{{ include "dognauts-starrocks.selectorLabels" . }}
app.kubernetes.io/component: fe
{{- end }}

{{/*
Префикс пути в шлюзе, без завершающего слеша
*/}}
{{- define "dognauts-starrocks.appPrefix" -}}
{{- .Values.istio.prefix | trimSuffix "/" }}
{{- end }}

{{/*
Admin UI
*/}}
{{- define "dognauts-starrocks.adminUiName" -}}
{{- printf "%s-admin-ui" (include "dognauts-starrocks.clusterName" .) }}
{{- end }}

{{- define "dognauts-starrocks.adminUiSelectorLabels" -}}
app.kubernetes.io/name: starrocks-admin-ui
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Префикс Admin UI: внутри префикса StarRocks, чтобы маска пути и политики Casbin были общими
*/}}
{{- define "dognauts-starrocks.adminUiPrefix" -}}
{{- printf "%s/admin-ui" (include "dognauts-starrocks.appPrefix" .) }}
{{- end }}

{{/*
ServiceAccount джоб, которые ходят в Vault. Имя по соглашению платформы *-vault-jobs —
этот шаблон внесён в роль Kubernetes-авторизации Vault (как у putSecretHook common-chart)
*/}}
{{- define "dognauts-starrocks.vaultJobsServiceAccount" -}}
{{- printf "%s-vault-jobs" .Release.Name }}
{{- end }}

{{/*
Ссылка на один ключ секрета в Vault для переменной окружения: vault:<путь>#<ключ>.
Webhook Bank-Vaults подставляет значение при старте контейнера. Каждый под получает только
нужные ему ключи (а не весь секрет, как при vault-env-from-path).
Параметры: dict "path" "key"
*/}}
{{- define "dognauts-starrocks.vaultRef" -}}
{{- printf "vault:%s#%s" .path .key | quote }}
{{- end }}

{{/*
Остановка sidecar Istio в конце джобы (иначе Job не завершится).
В образе может не быть curl — через /dev/tcp bash.
*/}}
{{- define "dognauts-starrocks.stopSidecar" -}}
stop_sidecar() {
  { exec 3<>/dev/tcp/127.0.0.1/15020 && printf 'POST /quitquitquit HTTP/1.0\r\n\r\n' >&3; } 2>/dev/null || true
}
trap stop_sidecar EXIT
{{- end }}
