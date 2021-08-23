# serum-crank

A performance and cost optimized serum crank based on the one from [serum-dex](https://github.com/project-serum/serum-dex/tree/master/dex/crank), that allows cranking multiple markets in a single transaction.

# Docker Image

A docker image that allows running the crank service through a docker container. Image is designed to be used with docker buildkit to enable as much caching as possible to reduce build times.  For those wishing to crank multiple markets at once, a docker compose file may be used.

## Requirements

* Requires a compatible docker installation with the "build kit" features enabled 
* requires you have the "experimental" features set to `true` in /etc/docker/daemon.json
* Requires `pigz` to compress the exported docker image

## Installation

```shell
$> make build-docker
```

Docker image will be available locally as `serum-crank:latest` or as a gzip compressed file in the current working directory as `serum_crank.tar.gz`

## Docker Compose

For use with docker compose and cranking multiple markets you can use `docker-compose.yml` as a base. 
    
