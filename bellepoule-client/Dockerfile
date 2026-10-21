# Client web BellePoule : compilation en WebAssembly (trunk), puis nginx.
FROM rust:1-bookworm AS build
ARG TRUNK_VERSION=0.21.14
RUN rustup target add wasm32-unknown-unknown \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin
WORKDIR /app
# Dépendances d'abord (couche mise en cache).
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
 && cargo build --release --target wasm32-unknown-unknown \
 && rm -rf src
COPY . .
RUN touch src/main.rs && trunk build --release

FROM nginx:1.27-alpine
# Adresse de l'API vue depuis ce conteneur (modifiable au lancement).
ENV BP_API_URL=http://api:8080
COPY nginx.conf /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
