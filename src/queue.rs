// (C) Copyright 2025 Hewlett Packard Enterprise Development LP
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
// THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.

use crate::memmanager::MemManager;

use std::{
    io::{self, Error, ErrorKind, Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

const SIZE_OFFSET: usize = 0;
const HEAD_OFFSET: usize = 4;
const TAIL_OFFSET: usize = 8;
const ELEM_OFFSET: usize = 12;
const SOCKET_NAME: &str = "notifsocket";

/// Returned when the notification socket peer has disconnected (BrokenPipe).
/// Matches the POSIX EPIPE errno value so C callers can check `ret == -EPIPE`.
pub const SHAREDQ_EPIPE: i32 = -32;

pub struct Queue {
    mem: MemManager,
    max_elems: usize,
    max_elem_size: usize,
    path: PathBuf,
    unix_stream: Option<UnixStream>,
    unix_listener: Option<UnixListener>,
}

struct ElemMeta {
    _pointer: u32,
    size: u32,
}

impl Queue {
    pub fn new(path: &Path, max_elems: usize, max_elem_size: usize) -> Result<Queue, Error> {
        let max_elems = max_elems + 1; // add one extra element to hold one empty elem because the tail points to last consumed element

        // size of meta is: max_elems(32) + head(32) + tail(32) + index*max_elem ( size_of(elem_meta) *max_elems)
        let meta_size: u64 = 4 + 4 + 4 + ((max_elems * core::mem::size_of::<ElemMeta>()) as u64);

        // size of arenas is: max_elems * max_elem_size
        let arenas_size: u64 = (max_elems * max_elem_size) as u64;

        let mut mem = MemManager::new(path, meta_size, arenas_size)?;

        mem.meta_write_u32(SIZE_OFFSET, max_elems as u32);

        let mut unix_stream: Option<UnixStream> = None;
        let mut unix_listener: Option<UnixListener> = None;

        let socket_initiable = attach_socket(path)?;
        match socket_initiable {
            SocketInitiable::Stream(val) => unix_stream = Some(val),
            SocketInitiable::Listener(val) => unix_listener = Some(val),
        }

        Ok(Queue {
            mem,
            max_elems,
            max_elem_size,
            path: path.to_path_buf(),
            unix_stream: unix_stream,
            unix_listener: unix_listener,
        })
    }

    /// Push a value onto the queue without blocking.
    ///
    /// Returns:
    /// - Positive value: number of bytes written (success)
    /// - 0: queue is full or element exceeds max size
    /// - SHAREDQ_EPIPE (-32): notification peer disconnected (BrokenPipe)
    pub fn push_non_blocking(&mut self, val: &[u8]) -> i32 {
        if val.len() > self.max_elem_size {
            println!("Unable to save element because is bigger than max element size configured!");
            return 0;
        }
        if self.is_full() {
            return 0;
        }

        let head = self.mem.meta_read_u32(HEAD_OFFSET);

        // next pointer to write
        let next_head = get_next_index(head, self.max_elems as u32);

        // Write in the arenas
        self.mem
            .arenas_write_bytes(self.max_elem_size * (next_head as usize), val);

        // Write the entry in the meta file
        let elem = ElemMeta {
            _pointer: next_head,
            size: val.len() as u32,
        };
        let elem_bytes = unsafe { any_as_u8_slice(&elem) };
        self.mem.meta_write_bytes(
            ELEM_OFFSET + ((next_head as usize) * core::mem::size_of::<ElemMeta>()),
            elem_bytes,
        );

        // write the head counter
        self.mem.meta_write_u32(HEAD_OFFSET, next_head);

        let notify_rc = self.notify(next_head);
        if notify_rc < 0 {
            return notify_rc;
        }

        val.len() as i32
    }

    pub fn is_empty(&mut self) -> bool {
        let head = self.mem.meta_read_u32(HEAD_OFFSET);
        let tail = self.mem.meta_read_u32(TAIL_OFFSET);
        head == tail
    }

    pub fn is_full(&mut self) -> bool {
        let head = self.mem.meta_read_u32(HEAD_OFFSET);
        let tail = self.mem.meta_read_u32(TAIL_OFFSET);

        if tail == get_next_index(head, self.max_elems as u32) {
            // it's full
            return true;
        }
        false
    }

    pub fn next_elem_size(&mut self) -> i32 {
        if self.is_empty() {
            return -1;
        }

        // tail is the last consumed element
        let mut tail = self.mem.meta_read_u32(TAIL_OFFSET);
        tail = get_next_index(tail, self.max_elems as u32);

        // read the value
        let mut elemmeta = ElemMeta {
            _pointer: 0,
            size: 0,
        };
        let elem_bytes = unsafe { any_as_u8_slice_mut(&mut elemmeta) };
        self.mem.meta_read_bytes(
            ELEM_OFFSET + ((tail as usize) * core::mem::size_of::<ElemMeta>()),
            elem_bytes,
        );

        elemmeta.size as i32
    }

    pub fn pop_non_blocking(&mut self) -> Vec<u8> {
        self.notify_clear();
        if self.is_empty() {
            return Vec::new();
        }

        // tail is the last consumed element
        let mut tail = self.mem.meta_read_u32(TAIL_OFFSET);
        tail = get_next_index(tail, self.max_elems as u32);

        // read the value
        let mut elemmeta = ElemMeta {
            _pointer: 0,
            size: 0,
        };
        let elem_bytes = unsafe { any_as_u8_slice_mut(&mut elemmeta) };
        self.mem.meta_read_bytes(
            ELEM_OFFSET + ((tail as usize) * core::mem::size_of::<ElemMeta>()),
            elem_bytes,
        );

        let mut elem = vec![0; elemmeta.size as usize];
        self.mem
            .arenas_read_bytes(self.max_elem_size * (tail as usize), &mut elem);

        self.mem.meta_write_u32(TAIL_OFFSET, tail);

        elem
    }

    pub fn reset(&mut self) {
        self.mem.meta_write_u32(HEAD_OFFSET, 0);
        self.mem.meta_write_u32(TAIL_OFFSET, 0);
    }

    pub fn socket_name(&self) -> String {
        String::from(self.path.join(SOCKET_NAME).to_str().unwrap())
    }

    pub fn socket_fd(&self) -> i32 {
        if let Some(stream) = &self.unix_stream {
            return stream.as_raw_fd();
        }
        return -1;
    }

    /// Notify the peer of a new value via the Unix socket.
    ///
    /// Returns 0 on success, SHAREDQ_EPIPE (-32) if the peer disconnected.
    fn notify(&mut self, val: u32) -> i32 {
        // If the socket stream is already initialized then use it
        if let Some(stream) = &mut self.unix_stream {
            if let Some(listener) = &self.unix_listener {
                reject_new_connections(listener);
            }
            match send_message(stream, val) {
                Ok(_) => 0,
                Err(ref e)
                    if e.kind() == ErrorKind::BrokenPipe
                        || e.kind() == ErrorKind::ConnectionReset
                        || e.kind() == ErrorKind::NotConnected =>
                {
                    // Peer disconnected — drop stale stream so the next
                    // notify() can accept a fresh connection via listener.
                    self.unix_stream = None;
                    SHAREDQ_EPIPE
                }
                Err(_) => SHAREDQ_EPIPE,
            }
        } else {
            // If the listener is already initialized then used it
            if let Some(listener) = &self.unix_listener {
                match accept_connection(listener) {
                    Some(stream) => {
                        self.unix_stream = Some(stream);
                        reject_new_connections(&listener);

                        self.notify(val)
                    }
                    None => 0, // can't notify because the other end is not connected
                }
            } else {
                panic!("at least the listener must be initialized")
            }
        }
    }

    fn notify_clear(&mut self) -> u32 {
        // If the socket stream is already initialized then use it
        if let Some(stream) = &mut self.unix_stream {
            if let Some(listener) = &self.unix_listener {
                reject_new_connections(listener);
            }
            match consume_message(stream) {
                Ok(val) => val,
                Err(ref e)
                    if e.kind() == ErrorKind::BrokenPipe
                        || e.kind() == ErrorKind::ConnectionReset
                        || e.kind() == ErrorKind::NotConnected
                        || e.kind() == ErrorKind::UnexpectedEof =>
                {
                    // Peer disconnected — drop stale stream for reconnection.
                    self.unix_stream = None;
                    0
                }
                Err(_) => {
                    self.unix_stream = None;
                    0
                }
            }
        } else {
            // If the listener is already initialized then used it
            if let Some(listener) = &self.unix_listener {
                match accept_connection(listener) {
                    Some(stream) => {
                        self.unix_stream = Some(stream);
                        reject_new_connections(listener);

                        self.notify_clear()
                    }
                    None => 0, // can't notify because the other end is not connected
                }
            } else {
                panic!("at least the listener must be initialized")
            }
        }
    }
}

/// Send a notification value over the socket.
/// Returns Ok(()) on success, or the io::Error on failure.
fn send_message(stream: &mut UnixStream, val: u32) -> io::Result<()> {
    stream.write_all(&val.to_be_bytes())
}

/// Read a notification value from the socket. Returns:
/// - Ok(val) on success (including 0 for WouldBlock/partial reads)
/// - Err(e) on I/O failure (BrokenPipe, ConnectionReset, EOF, etc.)
fn consume_message(stream: &mut UnixStream) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    match stream.read(&mut buf) {
        Ok(0) => Err(io::Error::new(ErrorKind::UnexpectedEof, "peer closed")),
        Ok(4) => Ok(u32::from_be_bytes(buf)),
        Ok(_) => Ok(0), // partial read — treat as no-op
        Err(ref e) if e.kind() == ErrorKind::WouldBlock => Ok(0),
        Err(e) => Err(e),
    }
}

enum SocketInitiable {
    Stream(UnixStream),
    Listener(UnixListener),
}

fn attach_socket(path: &Path) -> Result<SocketInitiable, Error> {
    let file = path.join(SOCKET_NAME);

    // Connect to file if exists
    if std::fs::metadata(&file).is_ok() {
        match UnixStream::connect(&file) {
            Ok(stream) => {
                stream.set_nonblocking(true).unwrap();
                Ok(SocketInitiable::Stream(stream))
            }
            Err(err) => match err.kind() {
                ErrorKind::ConnectionRefused => {
                    // Remove the file
                    std::fs::remove_file(&file)?;

                    // Start a listener
                    let unix_listener = UnixListener::bind(&file)?;
                    Ok(SocketInitiable::Listener(unix_listener))
                }
                _ => panic!("unable to handle error"),
            },
        }
    } else {
        // Create the socket
        let unix_listener = UnixListener::bind(file)?;
        Ok(SocketInitiable::Listener(unix_listener))
    }
}

fn accept_connection(listener: &UnixListener) -> Option<UnixStream> {
    listener.set_nonblocking(true).unwrap();

    match listener.accept() {
        Ok((stream, _addr)) => {
            stream.set_nonblocking(true).unwrap();
            Some(stream)
        }
        Err(err) => match err.kind() {
            io::ErrorKind::WouldBlock => None,
            _ => panic!("unexpected error"),
        },
    }
}

fn reject_new_connections(listener: &UnixListener) {
    listener.set_nonblocking(true).unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Immediately close newly accepted connection
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(_) => {
                break;
            }
        }
    }
}

fn get_next_index(val: u32, max: u32) -> u32 {
    (val + 1) % max
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    core::slice::from_raw_parts((p as *const T) as *const u8, core::mem::size_of::<T>())
}

unsafe fn any_as_u8_slice_mut<T: Sized>(p: &mut T) -> &mut [u8] {
    core::slice::from_raw_parts_mut((p as *mut T) as *mut u8, core::mem::size_of::<T>())
}

#[cfg(test)]
mod tests {
    use super::{Queue, SHAREDQ_EPIPE};
    use rand::random;
    use std::path::Path;
    use std::thread;

    #[test]
    fn test_push_pop() {
        const SIZE: usize = 4;
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let mut qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();

        let mut val: [u8; SIZE] = [0; SIZE];
        for index in 0..SIZE {
            val[index] = index as u8;
        }

        assert_eq!(true, qproducer.is_empty());
        let bytes_written = qproducer.push_non_blocking(&val);
        assert_eq!(SIZE as i32, bytes_written);
        assert_eq!(false, qproducer.is_empty());

        // Read the values from the queue
        let read = qconsumer.pop_non_blocking();
        assert_eq!(true, qproducer.is_empty());
        assert_eq!(val, &read[..]);
    }

    #[test]
    fn test_connect() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();

        let socket_fd = qconsumer.socket_fd();
        assert_ne!(-1, socket_fd);
    }

    #[test]
    fn test_push_full() {
        const SIZE: usize = 4;
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();
        assert_eq!(true, qproducer.is_empty());

        let mut val: [u8; SIZE] = [0; SIZE];
        for index in 0..SIZE {
            val[index] = index as u8;
        }

        for _index in 0..MAX_ELEMS {
            let bytes_written = qproducer.push_non_blocking(&val);
            assert_eq!(SIZE as i32, bytes_written);
        }

        assert_eq!(true, qproducer.is_full());

        let bytes_written = qproducer.push_non_blocking(&val);
        assert_eq!(0, bytes_written);
    }

    #[test]
    fn test_push_full_consume_all() {
        const SIZE: usize = 4;
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let mut qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();
        assert_eq!(true, qproducer.is_empty());

        let mut val: [u8; SIZE] = [0; SIZE];

        // fill the queue
        for index in 1..(MAX_ELEMS + 1) {
            val[0] = (index) as u8;
            let bytes_written = qproducer.push_non_blocking(&val);
            assert_eq!(SIZE as i32, bytes_written);
        }
        let bytes_written = qproducer.push_non_blocking(&val);
        assert_eq!(0, bytes_written);

        // consume all elements
        for index in 1..(MAX_ELEMS + 1) {
            let read = qconsumer.pop_non_blocking();
            val[0] = index as u8;
            assert_eq!(val, &read[..]);
        }
        assert_eq!(true, qproducer.is_empty());
        assert_eq!(false, qproducer.is_full());
    }

    #[test]
    fn test_concurrent_push_pop() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;
        const SEND_TRIES: usize = 1000000;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();
        assert_eq!(true, qproducer.is_empty());

        let mut values: Vec<Vec<u8>> = Vec::with_capacity(SEND_TRIES);
        for _i in 0..SEND_TRIES {
            let mut iv: Vec<u8> = Vec::with_capacity(MAX_ELEM_SIZE);
            for _j in 0..MAX_ELEM_SIZE {
                iv.push(random());
            }
            values.push(iv);
        }
        let values_copy = values.clone();

        let t0 = thread::spawn(move || {
            for i in 0..SEND_TRIES {
                loop {
                    let written = qproducer.push_non_blocking(&values[i]);
                    if written > 0 {
                        assert_eq!(MAX_ELEM_SIZE as i32, written);
                        break;
                    }
                }
            }
        });

        let t1 = thread::spawn(move || {
            let mut qconsumer =
                Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
            let mut counter = 0;

            loop {
                let read = qconsumer.pop_non_blocking();
                if read.len() != 0 {
                    assert_eq!(&values_copy[counter], &read[..]);
                    counter += 1;
                }
                if counter == SEND_TRIES {
                    break;
                }
            }
        });

        t0.join().unwrap();
        t1.join().unwrap();
    }

    #[test]
    fn test_socket_notify_socket_file_not_exists() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let _result = std::fs::remove_file("/tmp/qtest/notifsocket");

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let mut qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();

        for _i in 0..100 {
            let value = random();
            qproducer.notify(value);
            let received_value = qconsumer.notify_clear();
            assert_eq!(value, received_value);
        }
    }

    #[test]
    fn test_socket_notify_clear_before_notify() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let _ = std::fs::remove_file("/tmp/qtest/notifsocket");

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let mut qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();

        qconsumer.notify_clear();
    }

    #[test]
    fn test_socket_reuse_file() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        for i in 0..100 {
            let mut qproducer =
                Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
            let mut qconsumer =
                Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
            qproducer.reset();
            if (i % 2) == 0 {
                qconsumer.notify_clear();
            } else {
                qproducer.notify(random());
            }
        }
    }

    #[test]
    fn test_socket_old_msgs_discarded() {
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;

        let mut qproducer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        let mut qconsumer = Queue::new(Path::new("/tmp/qtest"), MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
        qproducer.reset();

        let mut values: [u32; MAX_ELEMS] = [0; MAX_ELEMS];
        for i in 0..MAX_ELEMS {
            values[i] = (i + 1) as u32;
            qproducer.notify(values[i]);
        }

        for i in 0..MAX_ELEMS {
            let received = qconsumer.notify_clear();
            assert_eq!(values[i], received);
        }

        // Reading when a message was not sent returns 0
        let received = qconsumer.notify_clear();
        assert_eq!(0, received);
    }

    #[test]
    fn test_push_returns_epipe_on_dead_consumer() {
        // Confirms that push_non_blocking() returns SHAREDQ_EPIPE (-32) when
        // the notification peer closes its socket (BrokenPipe).
        const MAX_ELEMS: usize = 8;
        const MAX_ELEM_SIZE: usize = 4;
        let path = Path::new("/tmp/qtest_epipe");

        let _ = std::fs::remove_file(path.join("notifsocket"));

        let val: [u8; MAX_ELEM_SIZE] = [1, 2, 3, 4];

        let mut qproducer = Queue::new(path, MAX_ELEMS, MAX_ELEM_SIZE).unwrap();

        {
            let _qconsumer = Queue::new(path, MAX_ELEMS, MAX_ELEM_SIZE).unwrap();
            qproducer.reset();

            // First push succeeds: producer accepts the connection and sends.
            let written = qproducer.push_non_blocking(&val);
            assert_eq!(MAX_ELEM_SIZE as i32, written);

            // _qconsumer drops here, closing its end of the socket.
        }

        // Second push hits BrokenPipe on notify() and must return SHAREDQ_EPIPE.
        let rc = qproducer.push_non_blocking(&val);
        assert_eq!(SHAREDQ_EPIPE, rc);
    }
}
