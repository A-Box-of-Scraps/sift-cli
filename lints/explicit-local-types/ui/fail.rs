#![allow(dead_code, unused_variables)]

use std::sync::Arc;

struct User;
trait Handler {}
impl Handler for User {}

fn load_user() -> Result<User, ()> {
    Ok(User)
}

fn main() -> Result<(), ()> {
    let user = load_user()?;
    let client = Arc::new(User);
    let handlers = vec![Box::new(User) as Box<dyn Handler>];
    let state = Some(User);
    let string = String::new();
    let reference = &user;
    let array = [1, 2];
    let slice = &array[..];
    let (left, right) = (User, User);
    let Some(value) = Some(User) else { return Ok(()) };
    let placeholder: _ = User;
    let partial: Vec<_> = vec![User];
    let borrowed: &Vec<_> = &vec![User];
    let bytes = b"foo";
    let delayed;
    delayed = User;
    let associated: Box<dyn Iterator<Item = _>> = Box::new(0..3);
    let function_pointer: fn() -> _ = || User;
    let const_placeholder: [u8; _] = [0; 3];
    Ok(())
}
