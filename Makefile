.PHONY: build-cli
build-cli:
	cargo build --release


.PHONY: build-docker
build-docker:
	DOCKER_BUILDKIT=1 docker \
		build \
		-t serum-crank:latest \
		--squash .
	docker image save serum-crank:latest -o serum_crank.tar
	pigz -f -9 serum_crank.tar
