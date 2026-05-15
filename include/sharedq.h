/*
 * (C) Copyright 2025 Hewlett Packard Enterprise Development LP
 *
 * Permission is hereby granted, free of charge, to any person obtaining a
 * copy of this software and associated documentation files (the "Software"),
 * to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense,
 * and/or sell copies of the Software, and to permit persons to whom the
 * Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included
 * in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR
 * OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
 * ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
 * OTHER DEALINGS IN THE SOFTWARE.
 */

#ifndef SHARED_Q
#define SHARED_Q

// Lets use some types which we can easily pair with rust types.
#include <stdint.h>

/**
 * Prints the version of the sharedq library to stdout.
 */
void version();

/**
 * Opaque type for a shared memory queue.
 */
typedef struct _queue Queue;

/**
 * Creates a new shared memory queue.
 * @param path Path to the directory for queue files.
 * @param max_elems Maximum number of elements in the queue.
 * @param max_elem_size Maximum size of each element in bytes.
 * @return Pointer to the created Queue, or NULL on failure.
 */
Queue* create_queue(const char*, uint32_t, uint32_t);

/**
 * Error code: notification peer disconnected (POSIX EPIPE).
 * Returned by push() when the socket peer closes unexpectedly.
 */
#define SHAREDQ_EPIPE (-32)

/**
 * Pushes a value into the queue in a non-blocking manner.
 * @param queue Pointer to the Queue.
 * @param val Pointer to the data to push.
 * @param size Size of the data in bytes.
 * @return Positive: bytes written. 0: queue full. SHAREDQ_EPIPE: peer disconnected.
 */
int32_t push(Queue*, const char*, uint);

/**
 * Resets the queue, clearing all elements.
 * @param queue Pointer to the Queue.
 */
void reset(Queue*);

/**
 * Checks if the queue is empty.
 * @param queue Pointer to the Queue.
 * @return 1 if empty, 0 otherwise.
 */
int32_t is_empty(Queue*);

/**
 * Checks if the queue is full.
 * @param queue Pointer to the Queue.
 * @return 1 if full, 0 otherwise.
 */
int32_t is_full(Queue*);

/**
 * Returns the size of the next element to pop, or -1 if the queue is empty.
 * @param queue Pointer to the Queue.
 * @return Size of the next element, or -1 if empty.
 */
int32_t pre_pop(Queue*);

/**
 * Pops the next element from the queue into the provided buffer.
 * @param queue Pointer to the Queue.
 * @param out Buffer to write the data.
 * @param size Size of the buffer (must match element size).
 * @return Number of bytes read, or -1 on error.
 */
int32_t pop(Queue*, char*, int32_t);

/**
 * Gets the socket file name used for notifications.
 * @param queue Pointer to the Queue.
 * @param out Buffer to write the socket file name.
 * @param size Size of the buffer.
 * @return Length of the file name written, or 0 on error.
 */
uint32_t socket_file(Queue*, char*, uint);

/**
 * Gets the file descriptor of the notification socket.
 * @param queue Pointer to the Queue.
 * @return File descriptor, or -1 if not available.
 */
int socket_fd(Queue*);

/**
 * Frees the memory associated with the queue.
 * @param queue Pointer to the Queue to free.
 */
void free_queue(Queue*);

#endif