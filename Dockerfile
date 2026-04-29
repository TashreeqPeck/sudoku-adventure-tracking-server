FROM node:22-alpine

RUN apk add --no-cache python3 make g++

WORKDIR /app

COPY package.json ./
# --omit=dev skips husky (devDependency), but `prepare` still runs `husky` → exit 127 if we do not
# skip lifecycle scripts. Rebuild native addons after install.
RUN npm install --omit=dev --ignore-scripts && npm rebuild better-sqlite3

COPY server.mjs ./
COPY public ./public

ENV NODE_ENV=production
ENV DATA_DIR=/data
ENV PORT=3840
ENV SHEET_SYNC_INTERVAL_MS=86400000

VOLUME ["/data"]
EXPOSE 3840

CMD ["node", "server.mjs"]
