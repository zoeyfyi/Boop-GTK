FROM rustembedded/cross:x86_64-unknown-linux-gnu

RUN dpkg --add-architecture amd64 && \
    apt-get update && \
    apt-get install --assume-yes libglib2.0-dev:amd64 libgtk-3-dev:amd64 libgtksourceview-3.0-dev:amd64
