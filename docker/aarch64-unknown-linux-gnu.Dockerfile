FROM rustembedded/cross:aarch64-unknown-linux-gnu

RUN dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install --assume-yes libglib2.0-dev:arm64 libgtk-3-dev:arm64 libgtksourceview-3.0-dev:arm64
