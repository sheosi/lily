# Do the build in a rust image (we'll move the binaries later)
FROM rust:1.58 as builder

# Dependencies and environment variables
RUN USER=root apt-get update && apt-get -y install libssl-dev libpocketsphinx-dev libsphinxbase-dev clang cmake
ENV LIBOPUS_STATIC=true

# With Rust in Docker the best way to proceed is to first build the dependencies
# (by setting up dummy projects) and then build the actual project.

# First, populate workspace with dummies
RUN USER=root \
    cargo new --bin lily && \
    cd lily && \
    cargo new --lib common && \
    cargo new --bin client

WORKDIR /lily

# Copy all project files
COPY ./common/Cargo.toml ./common/Cargo.toml
COPY ./client/Cargo.toml ./client/Cargo.toml
COPY ./Cargo.toml ./Cargo.toml

# Build the dependencies
RUN cargo build --release --package=lily


# Actual build
## Delete dummy sources
RUN rm src/*.rs
## Copy sources and build again
COPY . ./
RUN \
    rm ./target/release/deps/lily* && \
    cargo build --release --package=lily

# Move to final image and configure it
FROM debian:bullseye-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y libssl1.1 libpocketsphinx3 libsphinxbase3 \
    && rm -rf /var/lib/apt/lists/*

# Ports
## Unencrypted MQTT
EXPOSE 1883 
## Encrypted MQTT
EXPOSE 8883
## CoAP
EXPOSE 5683

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder \
    /lily/target/release/lily \
    ${APP}/lily
COPY resources ./resources

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./lily"]
