CREATE TABLE captures (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    serial_number     TEXT,
    ecid              TEXT NOT NULL,
    model_identifier  TEXT,
    name              TEXT NOT NULL,
    mode              TEXT NOT NULL,
    status            TEXT NOT NULL,
    timestamp_rfc3339 TEXT NOT NULL
);
