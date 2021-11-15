SELECT
  zome_name,
  scheduled_fn,
  maybe_schedule
FROM
  ScheduledFunctions
WHERE
  NOT ephemeral
  AND author = :author
  AND
END < :now
