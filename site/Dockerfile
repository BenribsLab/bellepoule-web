# Site public et documentation de ManageComp : génération avec Zola, puis
# service par nginx sans droits root.
FROM ghcr.io/getzola/zola:v0.21.0 AS build
COPY . /site
WORKDIR /site
RUN ["zola", "build", "--output-dir", "/site/public", "--force"]

FROM nginxinc/nginx-unprivileged:1.28-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /site/public /usr/share/nginx/html
EXPOSE 8080
