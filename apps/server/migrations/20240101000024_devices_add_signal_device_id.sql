-- Assign each device a stable, small, per-user Signal protocol device id.
--
-- The Signal ProtocolAddress is (user_id, device_id) where device_id is a u32,
-- and it keys the session store on both the sending and receiving side. A
-- client-local numbering would diverge between peers, so the server is the
-- single authority — mirroring Signal itself, where the primary device is 1 and
-- linked devices are 2, 3, ...

ALTER TABLE devices ADD COLUMN signal_device_id INTEGER;

-- Backfill existing rows: number each user's devices by creation order from 1.
WITH numbered AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at, id) AS n
    FROM devices
)
UPDATE devices
SET signal_device_id = numbered.n
FROM numbered
WHERE devices.id = numbered.id;

ALTER TABLE devices ALTER COLUMN signal_device_id SET NOT NULL;

-- Signal device ids must be positive; 0 is reserved/invalid in libsignal.
ALTER TABLE devices ADD CONSTRAINT devices_signal_device_id_positive
    CHECK (signal_device_id > 0);

-- Two devices of the same user must never share a signal device id, or their
-- sessions collide. This constraint is what makes concurrent registration safe:
-- a racing insert fails rather than silently aliasing two devices.
ALTER TABLE devices ADD CONSTRAINT devices_user_id_signal_device_id_key
    UNIQUE (user_id, signal_device_id);
