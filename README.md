# SharedQ: Shared Memory Queue with Notification

SharedQ is a high-performance, lock-free queue implemented in Rust, designed for inter-process communication (IPC) using shared memory. It supports notification via Unix domain sockets, making it suitable for producer-consumer scenarios across processes on the same machine.

## Features
- **Lock-free ring buffer** for fast, concurrent access
- **Shared memory** for zero-copy data transfer between processes
- **Notification mechanism** using Unix domain sockets
- **Configurable element size and queue length**
- **Non-blocking push and pop operations**
- **Automatic socket management and cleanup**
- **Comprehensive test suite**
- **C FFI compatibility**: Can be used from C, Go (via cgo), and any language that supports calling C functions

## Usage

### Creating a Queue
```rust
use sharedq::Queue;
use std::path::Path;

let mut queue = Queue::new(Path::new("/tmp/qtest"), 8, 256).unwrap();
queue.reset();
```

### Pushing Data
```rust
let data = vec![1, 2, 3, 4];
queue.push_non_blocking(&data);
```

### Popping Data
```rust
let received = queue.pop_non_blocking();
```

### Notifications
- Use `notify(val)` to send a notification value to another process.
- Use `notify_clear()` to receive and clear a notification.

## Building

This project uses Cargo (Rust's package manager):

```bash
cargo build --release
```

## Testing

Run the test suite with:

```bash
cargo test
```

## Generating Documentation

To generate and view the HTML documentation for the Rust library:

```bash
cargo doc --no-deps --open
```

This will build the documentation and open it in your default web browser. You can also open the documentation manually by visiting `target/doc/sharedq/index.html` after running:

```bash
cargo doc --no-deps
```

## Example (C Integration)

A C example is provided in the `example/` directory. You can build and run it as follows:

```bash
make
./example/main
```

## Example (Go Integration)

A Go example using cgo is provided in the `examples/` directory as `main.go`. You can review and run it to see how to use SharedQ from Go:

```bash
cd examples
# Build the Go example (ensure libsharedq.so is in your LD_LIBRARY_PATH)
go build -o goexample main.go
LD_LIBRARY_PATH=../target/release ./goexample
```
