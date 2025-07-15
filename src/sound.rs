use once_cell::sync::Lazy;
use std::io::Cursor;

pub static SOUND_NAMES: &[&str] = &[
    "None",
    "Alarm.wav",
    "Alarm02.wav",
    "Alarm03.wav",
    "Alarm04.wav",
    "Alarm05.wav",
    "Alarm06.wav",
    "Alarm07.wav",
    "Alarm08.wav",
    "Alarm09.wav",
    "AlarmNag.wav",
    "ReminderDelete.wav",
    "ReminderHold.wav",
    "ReminderStart.wav",
    "StartUp.wav",
];

static SOUNDS: Lazy<Vec<(&'static str, &'static [u8])>> = Lazy::new(|| {
    vec![
        ("Alarm.wav", include_bytes!("../Resources/sounds/Alarm.wav")),
        ("Alarm02.wav", include_bytes!("../Resources/sounds/Alarm02.wav")),
        ("Alarm03.wav", include_bytes!("../Resources/sounds/Alarm03.wav")),
        ("Alarm04.wav", include_bytes!("../Resources/sounds/Alarm04.wav")),
        ("Alarm05.wav", include_bytes!("../Resources/sounds/Alarm05.wav")),
        ("Alarm06.wav", include_bytes!("../Resources/sounds/Alarm06.wav")),
        ("Alarm07.wav", include_bytes!("../Resources/sounds/Alarm07.wav")),
        ("Alarm08.wav", include_bytes!("../Resources/sounds/Alarm08.wav")),
        ("Alarm09.wav", include_bytes!("../Resources/sounds/Alarm09.wav")),
        ("AlarmNag.wav", include_bytes!("../Resources/sounds/AlarmNag.wav")),
        (
            "ReminderDelete.wav",
            include_bytes!("../Resources/sounds/ReminderDelete.wav"),
        ),
        (
            "ReminderHold.wav",
            include_bytes!("../Resources/sounds/ReminderHold.wav"),
        ),
        (
            "ReminderStart.wav",
            include_bytes!("../Resources/sounds/ReminderStart.wav"),
        ),
        ("StartUp.wav", include_bytes!("../Resources/sounds/StartUp.wav")),
    ]
});

pub fn play_sound(name: &str) {
    if name == "None" {
        return;
    }
    let data = SOUNDS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, d)| *d);
    let Some(bytes) = data else { return; };
    if let Ok((_stream, handle)) = rodio::OutputStream::try_default() {
        if let Ok(source) = rodio::Decoder::new(Cursor::new(bytes)) {
            if let Ok(sink) = rodio::Sink::try_new(&handle) {
                sink.append(source);
                sink.detach();
            }
        }
    }
}
