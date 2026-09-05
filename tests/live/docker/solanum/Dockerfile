FROM debian:bookworm-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates git meson ninja-build gcc g++ pkg-config \
        libssl-dev libsqlite3-dev libltdl-dev python3 flex bison \
    && rm -rf /var/lib/apt/lists/*
ARG SOLANUM_REF=30f74b2cfa70aff2bad65c8afc62956d95699639
WORKDIR /src
RUN git init \
    && git remote add origin https://github.com/solanum-ircd/solanum.git \
    && git fetch --depth 1 origin ${SOLANUM_REF} \
    && git checkout --detach FETCH_HEAD
RUN meson setup build -Dprefix=/opt/solanum -Dopenssl=enabled \
    && meson compile -C build \
    && meson install -C build

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends libssl3 libsqlite3-0 libltdl7 netbase \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /opt/solanum --shell /usr/sbin/nologin solanum
COPY --from=build /opt/solanum /opt/solanum
RUN mkdir -p /opt/solanum/etc /opt/solanum/logs \
    && chown -R solanum:solanum /opt/solanum
USER solanum
WORKDIR /opt/solanum
EXPOSE 6667 6697
CMD ["/opt/solanum/bin/solanum", "-foreground", "-configfile", "/opt/solanum/etc/ircd.conf"]
