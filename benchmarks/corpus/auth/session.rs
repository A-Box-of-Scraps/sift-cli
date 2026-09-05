pub fn rotate_session_token(expired: bool) -> Result<(), String> {
    if expired { return Err("session token expired".into()); }
    Ok(())
}
