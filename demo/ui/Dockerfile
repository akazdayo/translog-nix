FROM golang:1.27.4-bookworm AS build
WORKDIR /src
COPY transparency-log/go.mod transparency-log/go.sum ./
RUN go mod download
COPY transparency-log/ ./
RUN go build -o /sync-log ./cmd/sync
FROM python:3.13-slim
COPY --from=build /sync-log /usr/local/bin/sync-log
COPY demo/ui/server.py /app/server.py
CMD ["python", "-u", "/app/server.py"]
