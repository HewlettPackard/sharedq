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

package main

/*
#cgo LDFLAGS: -lsharedq
#include "sharedq.h"
#include <stdlib.h>
*/
import "C"
import (
	"fmt"
	"unsafe"
)

func main() {
	path := C.CString("/tmp/qtest")
	defer C.free(unsafe.Pointer(path))
	queue := C.create_queue(path, 8, 256)
	defer C.free_queue(queue)

	data := []byte("hello")
	written := C.push(queue, (*C.char)(unsafe.Pointer(&data[0])), C.uint(len(data)))
	fmt.Println("Bytes written:", written)

	out := make([]byte, len(data))
	size := C.pre_pop(queue)
	if size > 0 {
		C.pop(queue, (*C.char)(unsafe.Pointer(&out[0])), size)
		fmt.Println("Popped:", string(out))
	}
}
