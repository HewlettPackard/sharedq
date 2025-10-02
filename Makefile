tests:
	cargo test -- --test-threads 1

build:
	cargo build --release

build-example: build
	gcc -Wall -g -O0 example/main.c -I. -Ltarget/release/ -lsharedq

run-example: build-example
	LD_LIBRARY_PATH=target/release ./a.out

clean:
	cargo clean
	rm -f a.out

install:
	cp include/sharedq.h /usr/include/sharedq.h
	cp target/release/libsharedq.so /usr/lib/libsharedq.so