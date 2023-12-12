pub struct RingBuffer<T: Clone + PartialEq> {
    buffer: Vec<Option<T>>,

    front: usize,
    back: usize,
}

impl<T: Clone + PartialEq> RingBuffer<T> {
    pub fn new(capacity: usize) -> RingBuffer<T> {
        RingBuffer { buffer: vec![None; capacity], front: 0, back: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    pub fn size(&self) -> usize {
        if self.is_full() {
            self.buffer.len()
        } else if self.back >= self.front {
            self.back - self.front
        } else {
            self.buffer.len() - self.front + self.back
        }
    }

    pub fn is_full(&self) -> bool {
        self.front == self.back && self.buffer[self.front].is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.front == self.back && self.buffer[self.front].is_none()
    }

    /* pub fn front(&self) -> &Option<T> {
        &self.buffer[self.front]
    } */

    pub fn push_back(&mut self, value: T) {
        if self.is_full() {
            self.front = (self.front + 1) % self.buffer.len();
        }

        self.buffer[self.back] = Some(value);
        self.back = (self.back + 1) % self.buffer.len();
    }

    pub fn find_and_push_back(&mut self, value: T) {
        if self.is_empty() {
            self.push_back(value);
        } else if let Some(index) = self.find_forwards(|x| *x == value, self.size() - 1) {
            let dst = if self.back == 0 {
                self.buffer.len()
            } else {
                self.back
            };

            let src = (self.front + index) % self.buffer.len();

            if src == dst - 1 {
            } else if src < dst {
                self.buffer[src..dst].rotate_left(1);
            } else {
                let cap = self.buffer.len();
                self.buffer[src..cap].rotate_left(1);
                self.buffer[0..dst].rotate_left(1);
                self.buffer.swap(cap - 1, dst - 1);
            }
        } else {
            self.push_back(value);
        }
    }

    /* pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            let item = self.buffer[self.front].take();
            self.front = (self.front + 1) % self.buffer.len();
            item
        }
    } */

    pub fn get(&self, index: usize) -> &Option<T> {
        &self.buffer[(self.front + index) % self.buffer.len()]
    }

    pub fn iter_from_back<'a>(&'a self) -> Box<dyn Iterator<Item = T> + 'a> {
        Box::new(RingBufferIterator::from(self, |x| x.saturating_sub(1)))
    }

    pub fn find_backwards(&self, pred: impl Fn(&T) -> bool, start_at: usize) -> Option<usize> {
        let mut iter = start_at;
        while let Some(value) = &self.get(iter) {
            if iter == self.buffer.len() {
                return None;
            }

            if pred(value) {
                return Some(iter);
            }

            iter += 1;
        }

        None
    }

    pub fn find_forwards(&self, pred: impl Fn(&T) -> bool, start_at: usize) -> Option<usize> {
        let mut iter = start_at;
        while let Some(value) = &self.get(iter) {
            if pred(value) {
                return Some(iter);
            }

            if iter == 0 {
                return None;
            }

            iter -= 1;
        }

        None
    }
}

pub struct RingBufferIterator<'a, T: Clone + PartialEq + 'a> {
    buffer: &'a RingBuffer<T>,
    position: usize,
    position_mut: fn(usize) -> usize,
    done: bool,
}

impl <'a, T: Clone + PartialEq + 'a> RingBufferIterator<'a, T> {
    fn from(buffer: &'a RingBuffer<T>, position_mut: fn(usize) -> usize) -> RingBufferIterator<'_, T> {
        RingBufferIterator {
            buffer,
            position: buffer.size().saturating_sub(1),
            position_mut,
            done: false,
        }
    }
}

impl<'a, T: Clone + PartialEq> Iterator for RingBufferIterator<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else if let Some(value) = self.buffer.get(self.position) {
            let new_position = (self.position_mut)(self.position);
            if new_position == self.position {
                self.done = true;
            }

            self.position = new_position;

            Some(value.clone())
        } else {
            None
        }
    }
}