INSTALL vss;
LOAD vss;
INSTALL fts;
LOAD fts;


CREATE TABLE IF NOT EXISTS {{table_name}} (
  uuid TEXT PRIMARY KEY,
  chunk TEXT NOT NULL,
  path TEXT,

  {% for vector, size in vectors %}
    {{vector}} FLOAT[{{size}}],
  {% endfor %}
);

