pub struct LazyZip<A,B> {
    a: A,
    b: Option<B>,
}

pub fn lazy_zip<A: Iterator, B: Iterator>(a: A, b: Option<B>) -> LazyZip<A,B> {
    LazyZip { a, b }
} 

impl<A: Iterator, B: Iterator> Iterator for LazyZip<A,B> {
    type Item = (A::Item, Option<B::Item>);

    fn next(&mut self) -> Option<Self::Item> {
        let a = self.a.next()?;
        let b = self.b.as_mut().and_then(|b| b.next());
        Some((a,b))
    }
}


pub struct Cache<T: Eq> {
    cache: Vec<T>,
    idx: usize,
    init: fn() -> T,
}

impl<T: Eq + Clone> Cache<T> {

    pub fn new(init: fn() -> T, cap: usize) -> Self {
        Cache { cache: Vec::with_capacity(cap), idx: 0, init }
    }

    pub fn send(&mut self) -> T {
        let id: T = (self.init)();
        let capa = self.cache.capacity();

        if self.cache.len() < capa {
            self.cache.push(id.clone())
        } else {
            self.cache[self.idx] = id.clone();
            self.idx += 1;
            if self.idx >= capa {
                self.idx = 0;
            }
        }
        id
    }

    pub fn sent(&mut self, id: T) -> bool {
        for found in &self.cache {
            if found == &id {
                return true;
            }
        }
        return false;
    }
}