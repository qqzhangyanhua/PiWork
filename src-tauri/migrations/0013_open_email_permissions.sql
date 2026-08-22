-- Reading and sending email are connector permissions, not per-action approvals.
-- Existing actions are authorized so interrupted Runs can continue after upgrade.
UPDATE connector_pending_actions
SET status = 'approved',
    resolved_at = COALESCE(resolved_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status IN ('pending', 'denied', 'cancelled', 'expired')
  AND action_type IN ('read_email_body', 'send_email');

UPDATE app_notifications
SET read_at = COALESCE(read_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    cleared_at = COALESCE(cleared_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
WHERE category = 'approval'
  AND json_extract(action_json, '$.approvalId') IN (
      SELECT id
      FROM connector_pending_actions
      WHERE action_type IN ('read_email_body', 'send_email')
  );
