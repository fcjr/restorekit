-- The full checkpoint messages the device reported during the restore, stored
-- as JSON arrays of strings (one element per checkpoint). `checkpoints_json` is
-- the compact-JSON view of each; `checkpoints_raw` is the exact plist as XML
-- (lossless). These are the device's self-reported operation log, not an
-- Apple-signed attestation. NULL for captures and rows logged before this
-- existed.
ALTER TABLE captures ADD COLUMN checkpoints_json TEXT;
ALTER TABLE captures ADD COLUMN checkpoints_raw TEXT;
