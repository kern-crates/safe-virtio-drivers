// [T; 128] do not implement `Default` trait, so wrap it
#[derive(Debug, Copy, Clone)]
pub(crate) struct Array<const SIZE: usize, T: Copy + Default> {
    pub(crate) data: [T; SIZE],
}
impl<const SIZE: usize, T: Copy + Default> Default for Array<SIZE, T> {
    fn default() -> Self {
        Self {
            data: [Default::default(); SIZE],
        }
    }
}
impl<const SIZE: usize, T: Copy + Default> From<[T; SIZE]> for Array<SIZE, T> {
    fn from(data: [T; SIZE]) -> Self {
        Self { data }
    }
}
impl<const SIZE: usize, T: Copy + Default> From<Array<SIZE, T>> for [T; SIZE] {
    fn from(data: Array<SIZE, T>) -> Self {
        data.data
    }
}
