INSERT INTO {{ table_name }} (uuid, chunk, path,  {{ vector_field_names | join(sep=", ") }})
VALUES (?, ?, ?,
  {% for _ in range(end=vector_field_names | length) %}
    ?,
  {% endfor %}
  )
ON CONFLICT (uuid) DO UPDATE SET
  chunk = EXCLUDED.chunk,
  path = EXCLUDED.path,
-- We cannot do true upserts in 1.1.1. This is supported in 1.2.0
  {% for vector in vector_field_names %}
    {{ vector }} = EXCLUDED.{{ vector }},
  {% endfor %}
;
