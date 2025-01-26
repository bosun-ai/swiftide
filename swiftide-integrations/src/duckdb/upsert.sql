INSERT INTO {{ table_name }} (uuid, chunk, path, metadata, original_size, offset, {{ vector_field_names | join(",") }})
VALUES (?, ?, ?, ?, ?, ?, 
  {% for _ in range(end=vector_field_names | len) %}
    ?,
  {% endfor %}
  )
) ON CONFINCT (uuid) DO UPDATE SET
  chunk = EXCLUDED.chunk,
  path = EXCLUDED.path,
  metadata = EXCLUDED.metadata,
  original_size = EXCLUDED.original_size,
  offset = EXCLUDED.offset,
  {% for vector in vector_field_names %}
    {{ vector }} = EXCLUDED.{{ vector }},
  {% endfor %}
  ;
