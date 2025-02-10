INSTALL vss;
LOAD vss;

CREATE TABLE IF NOT EXISTS {{table_name}} (
  uuid VARCHAR PRIMARY KEY,
  chunk VARCHAR NOT NULL,
  path VARCHAR,
  metadata MAP(VARCHAR, VARCHAR)
  original_size INT, 
  offset INT,

  -- NOTE mind want to add created / updated timestamps

  {% for vector, size in vectors %}
    {{vector}} FLOAT[size],
  {% endfor %}
);
