INSERT INTO {{ table_name }} (uuid, chunk, path, metadata, {{ vector_field_names | join(sep=", ") }})
VALUES (?, ?, ?, ?,
  {% for _ in range(end=vector_field_names | length) %}
    ?,
  {% endfor %}
  )
ON CONFLICT (uuid) DO UPDATE SET
  chunk = EXCLUDED.chunk,
  path = EXCLUDED.path,
  metadata = EXCLUDED.metadata,
  {% for vector in vector_field_names %}
    {{ vector }} = EXCLUDED.{{ vector }},
  {% endfor %}
  ;
