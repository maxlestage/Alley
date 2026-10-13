# Alley : un seul binaire, le serveur Paquets, avec le site et le navigateur
# (Rust compilé en WebAssembly) rangés dedans.
#
#   docker build -t alley . && docker run --rm -p 8080:8080 alley

FROM rust:1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown
WORKDIR /src
COPY . .
# Le navigateur (client/ → client/pkg), puis le serveur qui l'embarque.
RUN scripts/wasm-bindgen.sh && scripts/web.sh \
    && cargo build --release --bin alley && cp target/release/alley /alley

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 alley
COPY --from=build /alley /usr/local/bin/alley
USER alley
# Heroku fixe PORT au démarrage ; 8080 ailleurs.
ENV PORT=8080
EXPOSE 8080
CMD ["alley"]
