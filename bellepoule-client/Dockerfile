# Client web ManageComp : compilation en WebAssembly (trunk), puis nginx
# sans droits root.
FROM rust:1-trixie AS build
ARG TRUNK_VERSION=0.22.0
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
# Politique de sécurité : seul le script de démarrage généré par trunk
# (empreinte SHA-256) peut s'exécuter en ligne.
COPY csp.pl /app/csp.pl
RUN perl /app/csp.pl dist/index.html > /app/csp.conf && grep -q "sha256-" /app/csp.conf

FROM nginxinc/nginx-unprivileged:1.28-alpine
# Adresse de l'API vue depuis ce conteneur (modifiable au lancement).
ENV BP_API_URL=http://api:8080
COPY nginx.conf /etc/nginx/templates/default.conf.template
COPY security.conf /etc/nginx/security.conf
COPY --from=build /app/csp.conf /etc/nginx/csp.conf
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 8080
