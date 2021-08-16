# serum-crank

Docker image for serum crank using maximal rust compiler optimizations. See [serum's licese file](https://github.com/project-serum/serum-dex/blob/v0.4.0/LICENSE) for the license of the source code. 

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

## Configuration

* `NUMWORKERS` - defines the maximum workers for a crank
* `LOGDIRECTORY` - defines the file to store logs in, not the actual directory

## Docker Compose

For use with docker compose and cranking multiple markets you can use `docker-compose.yml` as a base. 

# Crank Docs

* anytime a series of events are received, one of the workers will "consume the events"
    * read keypair, clone the program id, get a client, clone accoune metas
    * optimizations: read keypair at startup and wrap it in an `Arc`
    
