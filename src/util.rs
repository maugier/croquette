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