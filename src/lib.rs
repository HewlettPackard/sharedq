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

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_uint};
use std::path::Path;
use std::slice;

mod memmanager;
mod queue;

#[repr(C)]
pub struct Queue {
    queue: queue::Queue,
}

/// Prints the version of the sharedq library to stdout.
#[no_mangle]
pub extern "C" fn version() {
    println!("sharedq version: {}", env!("CARGO_PKG_VERSION"));
}

/// Creates a new shared memory queue.
///
/// # Arguments
/// * `path` - Path to the directory for queue files (as C string).
/// * `max_elems` - Maximum number of elements in the queue.
/// * `max_elem_size` - Maximum size of each element in bytes.
///
/// # Returns
/// Pointer to the created Queue, or null on failure.
#[no_mangle]
pub extern "C" fn create_queue(
    path: *const c_char,
    max_elems: u32,
    max_elem_size: u32,
) -> *mut Queue {
    let c_str = unsafe { CStr::from_ptr(path) };

    Box::into_raw(Box::new(Queue {
        queue: queue::Queue::new(
            Path::new(c_str.to_str().unwrap()),
            max_elems as usize,
            max_elem_size as usize,
        )
        .unwrap(),
    }))
}

/// Pushes a value into the queue in a non-blocking manner.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
/// * `val` - Pointer to the data to push.
/// * `size` - Size of the data in bytes.
///
/// # Returns
/// Number of bytes written, or 0 if the queue is full or on error.
#[no_mangle]
pub extern "C" fn push(q: &mut Queue, val: *const c_uchar, size: c_uint) -> u32 {
    let slice = unsafe { slice::from_raw_parts(val, size as usize) };
    q.queue.push_non_blocking(slice)
}

/// Resets the queue, clearing all elements.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
#[no_mangle]
pub extern "C" fn reset(q: &mut Queue) {
    q.queue.reset()
}

/// Frees the memory associated with the queue.
///
/// # Arguments
/// * `q` - Pointer to the Queue to free.
#[no_mangle]
pub extern "C" fn free_queue(q: *mut Queue) {
    if q.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(q));
    }
}

/// Checks if the queue is empty.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
///
/// # Returns
/// 1 if empty, 0 otherwise.
#[no_mangle]
pub extern "C" fn is_empty(q: &mut Queue) -> i32 {
    if q.queue.is_empty() {
        return 1;
    } else {
        return 0;
    }
}

/// Checks if the queue is full.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
///
/// # Returns
/// 1 if full, 0 otherwise.
#[no_mangle]
pub extern "C" fn is_full(q: &mut Queue) -> i32 {
    if q.queue.is_full() {
        return 1;
    } else {
        return 0;
    }
}

/// Returns the size of the next element to pop, or -1 if the queue is empty.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
///
/// # Returns
/// Size of the next element, or -1 if empty.
#[no_mangle]
pub extern "C" fn pre_pop(q: &mut Queue) -> i32 {
    q.queue.next_elem_size()
}

/// Pops the next element from the queue into the provided buffer.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
/// * `out` - Pointer to the buffer to write the data.
/// * `size` - Size of the buffer (must match element size).
///
/// # Returns
/// Number of bytes read, or -1 on error.
#[no_mangle]
pub extern "C" fn pop(q: &mut Queue, out: *mut c_uchar, size: c_int) -> i32 {
    let expected_size = q.queue.next_elem_size();
    if size != expected_size {
        return -1;
    }

    let elem = q.queue.pop_non_blocking();
    let out_s = unsafe { slice::from_raw_parts_mut(out, size as usize) };
    out_s.copy_from_slice(&elem);
    expected_size
}

/// Gets the socket file name used for notifications.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
/// * `out` - Buffer to write the socket file name.
/// * `size` - Size of the buffer.
///
/// # Returns
/// Length of the file name written, or 0 on error.
#[no_mangle]
pub extern "C" fn socket_file(q: &mut Queue, out: *mut c_uchar, size: c_uint) -> u32 {
    let filename = q.queue.socket_name();
    if filename.len() > (size as usize) {
        return 0;
    }

    let out_s = unsafe { slice::from_raw_parts_mut(out, filename.len()) };
    out_s.copy_from_slice(filename.as_bytes());

    filename.len() as u32
}

/// Gets the file descriptor of the notification socket.
///
/// # Arguments
/// * `q` - Pointer to the Queue.
///
/// # Returns
/// File descriptor, or -1 if not available.
#[no_mangle]
pub extern "C" fn socket_fd(q: &mut Queue) -> c_int {
    q.queue.socket_fd()
}
