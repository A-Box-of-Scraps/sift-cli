#![allow(dead_code, unused_variables)]

use std::sync::Arc;

struct User;
trait Handler {}
impl Handler for User {}

fn opaque() -> impl Iterator<Item = u8> {
    0..3
}

macro_rules! generated {
    () => {
        let generated = String::new();
    };
}

fn main() {
    let mut start = 0;
    let enabled = true;
    let name = "foo";
    let character = 'x';
    let float = 1.5;
    let unit = ();
    let user: User = User;
    let client: Arc<User> = Arc::new(User);
    let handlers: Vec<Box<dyn Handler>> = vec![Box::new(User)];
    let state: Option<User> = Some(User);
    let (left, right): (User, User) = (User, User);
    let Some(value): Option<User> = state else { return };
    let closure = || User;
    let mapped = (0..3).map(|_| User);
    let iterator = opaque();
    let function = opaque;
    let future = async { User };
    let _ = User;
    if let Some(user) = Some(User) {}
    while let Some(user) = None::<User> {}
    for user in [User] {}
    generated!();
    #[allow(explicit_local_types)]
    let allowed = User;
}
