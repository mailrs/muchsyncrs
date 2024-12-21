pub trait CallbackFn<T>: Fn(T) + Send {
    fn clone_box(&self) -> Box<dyn CallbackFn<T>>;
}

impl<T, F> CallbackFn<T> for F
where
    F: Fn(T) + Clone + Send + 'static,
{
    fn clone_box(&self) -> Box<dyn CallbackFn<T>> {
        Box::new(self.clone())
    }
}

impl<T: 'static> Clone for Box<dyn CallbackFn<T>> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}
