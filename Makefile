.PHONY: build-cli
build-cli:
	cargo build --release
	cp target/release/crank crank


.PHONY: build-docker
build-docker:
	DOCKER_BUILDKIT=1 docker \
		build \
		-t serum-crank:latest \
		--squash .
	docker image save serum-crank:latest -o serum_crank.tar
	pigz -f -9 serum_crank.tar

.PHONY: fmt
fmt:
	find -type f -name "*.rs" -not -path "*target*" -exec rustfmt --edition 2018 {} \;
