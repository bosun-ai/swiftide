INSTALL vss;
LOAD vss;

CREATE TABLE IF NOT EXISTS {{table_name}} (
  uuid TEXT PRIMARY KEY,
  chunk TEXT NOT NULL,
  path TEXT,

  {% for vector, size in vectors %}
    {{vector}} FLOAT[{{size}}],
  {% endfor %}
);

{% for vector, _ in vectors %}
CREATE INDEX {{table_name}}_{{vector}} ON {{table_name}} USING HNSW ({{vector}});
{% endfor %}
