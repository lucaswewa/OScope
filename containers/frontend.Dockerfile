# syntax=docker/dockerfile:1
FROM docker:29.1.3-cli@sha256:e0b6ff45302985d99b9685b2a2fec50c3379bd47067493968f51da901dc53ad5 AS docker-cli
FROM node:24.21.0-bookworm-slim@sha256:51b1100cc2a83d370c6a60952e3f2989c8a43159d0e38586e090f3b3326efefd AS frontend
RUN apt-get update && apt-get install -y --no-install-recommends git python3 ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /workspace/web/node_modules /home/node/.npm \
    && chown -R node:node /workspace /home/node/.npm
ENV HOME=/home/node NPM_CONFIG_CACHE=/home/node/.npm
USER node
WORKDIR /workspace/web
CMD ["bash"]

FROM frontend AS integration
USER root
COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --chown=node:node web/package.json web/package-lock.json ./
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
RUN --mount=type=cache,id=oscope-browser-npm,target=/home/node/.npm \
    npm ci --no-audit --no-fund \
    && npx playwright install --with-deps chromium firefox webkit \
    && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends xauth \
    && rm -rf /var/lib/apt/lists/*
USER node
