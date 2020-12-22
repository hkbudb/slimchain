use core::{iter::Iterator, result::Result};

pub struct IterResult<'a, T, E, I>
where
    I: Iterator<Item = Result<T, E>>,
{
    input: I,
    err: &'a mut Option<E>,
}

impl<'a, T, E, I> Iterator for IterResult<'a, T, E, I>
where
    I: Iterator<Item = Result<T, E>>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.err.is_some() {
            return None;
        }

        match self.input.next() {
            Some(Ok(item)) => Some(item),
            Some(Err(e)) => {
                self.err.replace(e);
                None
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.err.is_some() {
            (0, Some(0))
        } else {
            self.input.size_hint()
        }
    }
}

pub fn iter_result<T, U, E, I, F>(input: I, func: F) -> Result<U, E>
where
    I: Iterator<Item = Result<T, E>>,
    F: FnOnce(IterResult<'_, T, E, I>) -> U,
{
    let mut err = None;
    let out = func(IterResult {
        input,
        err: &mut err,
    });

    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

pub struct IterResultRef<'a, 'b, T, E, I>
where
    T: 'a,
    E: 'a + Clone,
    I: Iterator<Item = &'a Result<T, E>>,
{
    input: I,
    err: &'b mut Option<E>,
}

impl<'a, 'b, T, E, I> Iterator for IterResultRef<'a, 'b, T, E, I>
where
    T: 'a,
    E: 'a + Clone,
    I: Iterator<Item = &'a Result<T, E>>,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.err.is_some() {
            return None;
        }

        match self.input.next() {
            Some(Ok(item)) => Some(item),
            Some(Err(e)) => {
                self.err.replace(e.clone());
                None
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.err.is_some() {
            (0, Some(0))
        } else {
            self.input.size_hint()
        }
    }
}

pub fn iter_result_ref<'a, T, U, E, I, F>(input: I, func: F) -> Result<U, E>
where
    T: 'a,
    E: 'a + Clone,
    I: Iterator<Item = &'a Result<T, E>>,
    F: FnOnce(IterResultRef<'a, '_, T, E, I>) -> U,
{
    let mut err = None;
    let out = func(IterResultRef {
        input,
        err: &mut err,
    });

    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn test_iter_result() {
        let input1: Vec<Result<i32, i32>> = vec![Ok(1), Ok(2), Err(0), Ok(3)];
        let input2: Vec<Result<i32, i32>> = vec![Ok(1), Ok(2), Ok(3)];

        assert_eq!(
            iter_result(input1.into_iter(), |iter| iter.sum::<i32>()),
            Err(0)
        );
        assert_eq!(
            iter_result(input2.into_iter(), |iter| iter.sum::<i32>()),
            Ok(6)
        );
    }

    #[test]
    fn test_iter_result_ref() {
        let input1: Vec<Result<i32, i32>> = vec![Ok(1), Ok(2), Err(0), Ok(3)];
        let input2: Vec<Result<i32, i32>> = vec![Ok(1), Ok(2), Ok(3)];

        assert_eq!(
            iter_result_ref(input1.iter(), |iter| iter.sum::<i32>()),
            Err(0)
        );
        assert_eq!(
            iter_result_ref(input2.iter(), |iter| iter.sum::<i32>()),
            Ok(6)
        );
    }
}
