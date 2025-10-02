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

extern crate memmap2;

use memmap2::MmapMut;
use std::{
    fs::{self, OpenOptions},
    io::Error,
    path::Path,
};

pub struct MemManager {
    meta: MmapMut,   // memory to store the pointers, head and tail
    arenas: MmapMut, // memory to temporaly store the packets
}

impl MemManager {
    pub fn new(path: &Path, meta_size: u64, arenas_size: u64) -> Result<MemManager, Error> {
        if path.exists() {
            if !path.is_dir() {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    format!("{} is not a dir", path.to_string_lossy()),
                ));
            }
        } else {
            fs::create_dir_all(path)?;
        }

        // Metafile
        let meta_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path.join("meta.qmem"))?;
        meta_file.set_len(meta_size)?;
        let meta_mmap = unsafe { MmapMut::map_mut(&meta_file) }?;

        // Arenas
        let arenas_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path.join("arenas.qmem"))?;
        arenas_file.set_len(arenas_size)?;
        let arena_mmap = unsafe { MmapMut::map_mut(&arenas_file) }?;

        Ok(MemManager {
            meta: meta_mmap,
            arenas: arena_mmap,
        })
    }

    pub fn meta_read_u32(&mut self, offset: usize) -> u32 {
        let v = self.meta.get(offset..(offset + 4)).expect("cannot be none");
        u32::from_be_bytes(v.try_into().expect("cannot to converto to array"))
    }

    pub fn meta_read_bytes(&mut self, offset: usize, target: &mut [u8]) {
        let bytes = &self.meta[offset..(offset + target.len())];
        target.clone_from_slice(bytes);
    }

    pub fn meta_write_u32(&mut self, offset: usize, val: u32) {
        let bytes = val.to_be_bytes();
        let subslice = &mut self.meta[offset..(offset + 4)];
        subslice.copy_from_slice(&bytes);
    }

    pub fn meta_write_bytes(&mut self, offset: usize, val: &[u8]) {
        let subslice = &mut self.meta[offset..(offset + val.len())];
        subslice.copy_from_slice(val);
    }

    pub fn arenas_write_bytes(&mut self, offset: usize, val: &[u8]) {
        let subslice = &mut self.arenas[offset..(offset + val.len())];
        subslice.copy_from_slice(val);
    }

    pub fn arenas_read_bytes(&self, offset: usize, target: &mut [u8]) {
        let bytes = &self.arenas[offset..(offset + target.len())];
        target.clone_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::MemManager;

    #[test]
    fn test_tail_head() {
        let mut mem =
            MemManager::new(Path::new("/tmp/qtest"), 8, 5120).expect("error creating mem");

        mem.meta_write_u32(0, 0);
        mem.meta_write_u32(4, 0);

        assert_eq!(mem.meta_read_u32(0), 0);
        assert_eq!(mem.meta_read_u32(4), 0);
        mem.meta_write_u32(0, 1);
        assert_eq!(mem.meta_read_u32(0), 1);

        let mut mem2 =
            MemManager::new(Path::new("/tmp/qtest"), 8, 5120).expect("error creating mem");
        assert_eq!(mem2.meta_read_u32(0), 1);
    }
}
