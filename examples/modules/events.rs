use std::sync::{Mutex, Once, OnceLock};

static INIT: Once = Once::new();
static NEXT_ID: OnceLock<Mutex<i32>> = OnceLock::new();

fn next_id() -> i32 {
    let guard = NEXT_ID.get_or_init(|| Mutex::new(1));
    let mut value = guard.lock().unwrap();
    let id = *value;
    *value += 1;
    id
}

pub fn create_user(email: String) -> UserCreated {
    INIT.call_once(|| {
        crate::events::on_user_created(|ev| {
            println!("event: user created {} {}", ev.id, ev.email);
        });
    });
    let user = UserCreated {
        id: next_id(),
        email,
    };
    crate::events::emit_user_created(&user);
    user
}
