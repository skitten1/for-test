FROM harbor.neoflex.ru/dognauts/node:22-bookworm

ENV NPM_CONFIG_CACHE=/tmp/npm-cache
RUN mkdir -p /tmp/npm-cache && chmod -R 777 /tmp/npm-cache
WORKDIR /tmp/npmwarm
RUN npm install axios lodash date-fns

RUN groupadd -r -g 64535 sandbox 2>/dev/null || true \
    && useradd -r -g sandbox -u 64535 sandbox 2>/dev/null || true

RUN chmod -R a+rX /tmp/npm-cache

# Работать строго офлайн — не пытаться дотянуться в сеть за пакетами в рантайме
ENV NPM_CONFIG_OFFLINE=true
ENV NPM_CONFIG_PREFER_OFFLINE=true

USER 64535