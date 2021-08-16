# if you need to use this image with ssh keys for private git repo access uncommented the relevant lines
# built with `--squash` otheriwse you will leak your keys
FROM rust:1.54.0 as BUILDER
RUN apt-get update && apt-get install -y libudev-dev
RUN cargo install sccache
ENV HOME=/home/root
ENV SCCACHE_CACHE_SIZE="2G"
ENV SCCACHE_DIR=$HOME/.cache/sccache
ENV RUSTC_WRAPPER="/usr/local/cargo/bin/sccache"
WORKDIR $HOME/app
# Copy all files into the docker image
ADD . .
# Uncomment if you need to incldue ssh keys
# Create the ssh directory, copy relevant files, authorize ssh host, and change permission
# RUN mkdir -p /root/.ssh && \
#     chmod 0700 /root/.ssh && \
#     cp ssh/id_rsa /root/.ssh && \
#     cp ssh/id_rsa.pub /root/.ssh && \
#     ssh-keyscan github.com > /root/.ssh/known_hosts && \
#     chmod 600 /root/.ssh/id_rsa && \
#     chmod 600 /root/.ssh/id_rsa.pub
# # Start the ssh mount
# RUN --mount=type=ssh ssh -q -T git@github.com 2>&1 | echo "started docker ssh mount"
# Start the cache mount and build the cli
RUN --mount=type=cache,target=/home/root/.cache/sccache cargo build --release --bin crank && cp target/release/crank /tmp/crank
# Shred the ssh keys and remove them
# RUN shred /root/.ssh/id_rsa && shred /root/.ssh/id_rsa.pub && rm -rf /root/.ssh
FROM rust:1.54.0 as runtime
COPY --from=BUILDER /tmp/crank /usr/local/bin
COPY --from=BUILDER /home/root/app/entrypoint.sh /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]