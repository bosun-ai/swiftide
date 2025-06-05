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

PRAGMA create_fts_index('{{table_name}}', 'uuid', 'chunk', stemmer = 'porter',
                 stopwords = 'english', ignore = '(\\.|[^a-z])+',
                 strip_accents = 1, lower = 1, overwrite = 0);
