//! Local development adapter for the same pure in-process functions.
use std::io::{self, Write};
fn main() -> io::Result<()> {
    let fixture = std::env::args().nth(1).as_deref() == Some("--notification-fixture");
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut frame = Vec::new();
    while seele_runtime::wire::read_frame(&mut input, &mut frame, seele_qml_core::MAX_MESSAGE)? {
        output.write_all(&if fixture {
            seele_qml_core::notification_fixture(&frame)
        } else {
            seele_qml_core::evaluate(&frame)
        })?;
        output.flush()?;
    }
    Ok(())
}
