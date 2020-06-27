FROM rustembedded/cross:x86_64-pc-windows-gnu

RUN apt-get update && \
    apt-get install --assume-yes libglib2.0-dev libgtk-3-dev libgtksourceview-3.0-dev ninja-build clang

RUN git clone https://gn.googlesource.com/gn && cd gn && python build/gen.py && ninja -C out

RUN cp gn/out/gn /usr/bin

