-- The parts of NoAr an operator may use, as a JSON array of area names
-- (player, playlist, schedule, graphics, storage, report, log). NULL means
-- every area, so existing operators keep the access they have.
ALTER TABLE auth_user ADD COLUMN areas TEXT;
