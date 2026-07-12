-- Records the encryption-key obliteration verdict for an erase restore:
-- 'confirmed', 'failed', 'unconfirmed', or 'not_applicable' (see
-- restore::Obliteration). NULL for rows logged before this column existed and
-- for non-restore captures.
ALTER TABLE captures ADD COLUMN obliteration TEXT;
