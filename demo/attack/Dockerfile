FROM golang:1.27.4-bookworm AS build
WORKDIR /src
COPY demo/attack/sign.go ./
RUN CGO_ENABLED=0 go build -o /sign sign.go
FROM nixos/nix:latest
COPY --from=build /sign /usr/local/bin/demo-sign
COPY demo/attack/prepare.sh /usr/local/bin/prepare-attack
ENTRYPOINT ["sh", "/usr/local/bin/prepare-attack"]
