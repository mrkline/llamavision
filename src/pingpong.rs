use atomic_wait as futex;
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

const WRITE_INDEX_BIT: u32 = 0;
const READY_BIT: u32 = 1;

fn write_index(s: u32) -> usize {
    ((s >> WRITE_INDEX_BIT) & 1) as usize
}

fn read_index(s: u32) -> usize {
    (((s >> WRITE_INDEX_BIT) & 1) as usize + 1) % 2
}

fn swap_indexes(s: u32) -> u32 {
    let i = (s >> WRITE_INDEX_BIT) & 1;
    let ni = (i + 1) % 2;
    (s & !(1 << WRITE_INDEX_BIT)) | (ni << WRITE_INDEX_BIT)
}

fn is_ready(s: u32) -> bool {
    s & (1 << READY_BIT) != 0
}

fn set_ready(s: u32) -> u32 {
    s | (1 << READY_BIT)
}

fn clear_ready(s: u32) -> u32 {
    s & !(1 << READY_BIT)
}

struct PingPong<T> {
    bufs: [UnsafeCell<T>; 2],
    sync: AtomicU32,
}

unsafe impl<T> Sync for PingPong<T> {}

impl<T: Clone> PingPong<T> {
    pub fn new(initial: T) -> Self {
        let b1 = initial;
        let b2 = b1.clone();
        Self {
            bufs: [UnsafeCell::new(b1), UnsafeCell::new(b2)],
            sync: AtomicU32::new(0),
        }
    }

    fn write<F: FnOnce(&mut T)>(&self, f: F) -> bool {
        let s = self.sync.load(Ordering::Acquire);

        // Writers ain't got no time to wait for a slow reader.
        // (Slightly fancier would be to make this a try_write(),
        // and have write() sleep, awoken when the reader clears the ready bit.)
        if is_ready(s) {
            return false;
        }
        // SAFETY: The reader is looking at the other guy. Write this one...
        f(unsafe { self.bufs[write_index(s)].get().as_mut().unwrap() });

        // Then swap and wake any reader.
        self.sync
            .store(set_ready(swap_indexes(s)), Ordering::Release);
        futex::wake_one(&self.sync);
        true
    }

    fn read<F: FnOnce(&T)>(&self, f: F) {
        // Wait for when the writer has something for us to read.
        let mut s;
        loop {
            s = self.sync.load(Ordering::Acquire);
            if is_ready(s) {
                // Immediately clear the ready bit so the writer can start writing
                // to the other slot.
                self.sync.store(clear_ready(s), Ordering::Release);
                break;
            }
            futex::wait(&self.sync, s);
        }
        // SAFETY: The writer always writes to opposite index. One can't outpace
        //         the other since the writer will not write a second item
        //         before the reader reads one..
        //         (See the sync dance on the ready bit above.)
        f(unsafe { self.bufs[read_index(s)].get().as_ref().unwrap() });
    }
}

pub struct PingPongWriter<T> {
    inner: Arc<PingPong<T>>,
}

impl<T: Clone> PingPongWriter<T> {
    pub fn write<F: FnOnce(&mut T)>(&self, f: F) -> bool {
        self.inner.write(f)
    }
}

pub struct PingPongReader<T> {
    inner: Arc<PingPong<T>>,
}

impl<T: Clone> PingPongReader<T> {
    pub fn read<F: FnOnce(&T)>(&self, f: F) {
        self.inner.read(f)
    }
}

pub fn ping_pong<T: Clone>(init: T) -> (PingPongWriter<T>, PingPongReader<T>) {
    let p1 = Arc::new(PingPong::new(init));
    let p2 = p1.clone();
    let w = PingPongWriter { inner: p1 };
    let r = PingPongReader { inner: p2 };
    (w, r)
}
