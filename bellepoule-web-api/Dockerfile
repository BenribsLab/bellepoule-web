# API web BellePoule.
FROM rust:1-bookworm AS build
WORKDIR /app
# Dépendances d'abord (couche mise en cache).
COPY Cargo.toml Cargo.lock* ./
COPY bp-core/Cargo.toml bp-core/Cargo.toml
COPY bp-net/Cargo.toml bp-net/Cargo.toml
COPY marshaller/Cargo.toml marshaller/Cargo.toml
RUN mkdir -p src bp-core/src bp-net/src marshaller/src && echo "fn main() {}" > src/main.rs \
 && touch bp-core/src/lib.rs bp-net/src/lib.rs marshaller/src/lib.rs \
 && cargo build --release \
 && rm -rf src bp-core/src bp-net/src marshaller/src
COPY . .
RUN touch src/main.rs bp-core/src/lib.rs bp-net/src/lib.rs marshaller/src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --home /app bellepoule
WORKDIR /app
COPY --from=build /app/target/release/bellepoule-web-api /usr/local/bin/bellepoule-web-api
COPY resources /app/resources
RUN mkdir -p /data && chown bellepoule /data
USER bellepoule
# Heure locale de la salle (pistes, frise) : fuseau du club.
ENV TZ=Europe/Paris \
    BP_DATA_DIR=/data \
    BP_RESOURCES=/app/resources \
    BP_LANGUAGE=fr \
    BP_ADDRESS=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fs http://localhost:8080/api/health || exit 1
CMD ["bellepoule-web-api"]
