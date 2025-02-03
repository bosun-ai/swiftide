INSTALL vss;
LOAD vss;

CREATE TABLE IF NOT EXISTS {{table_name}} (
  uuid VARCHAR PRIMARY KEY,
  chunk VARCHAR NOT NULL,
  path VARCHAR,

  {% for vector, size in vectors %}
    {{vector}} FLOAT[{{size}}],
  {% endfor %}
);
