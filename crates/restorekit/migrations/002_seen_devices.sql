CREATE TABLE seen_devices (
    ecid              TEXT PRIMARY KEY,
    serial_number     TEXT,
    model_identifier  TEXT,
    name              TEXT NOT NULL,
    chip              TEXT,
    board             TEXT,
    mode              TEXT NOT NULL,
    port              TEXT,
    first_seen        TEXT NOT NULL,
    last_seen         TEXT NOT NULL
);
