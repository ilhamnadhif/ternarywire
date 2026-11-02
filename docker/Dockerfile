# TernaryWire CLI image (FR-L3).
#   docker build -f docker/Dockerfile -t ternarywire .
#   docker run --rm -v "$PWD:/work" ternarywire run collection.json -e env.json -r cli,junit
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --profile dist --locked -p ternarywire-cli --bin ternarywire \
    && strip target/dist/ternarywire

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1000 ternarywire
COPY --from=build /src/target/dist/ternarywire /usr/local/bin/ternarywire
USER ternarywire
WORKDIR /work
# No keychain in containers: workspace secrets come from .ternarywire/secrets.json.
ENV TERNARYWIRE_NO_KEYCHAIN=1
ENTRYPOINT ["ternarywire"]
CMD ["--help"]
