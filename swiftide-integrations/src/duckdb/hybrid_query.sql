with scored as (
    select
        uuid,
        chunk,
        path,
        fts_main_{{table_name}}.match_bm25(uuid, {{query}}, fields := 'chunk') as raw_fts_score,
        array_cosine_similarity({{embedding_name}}, cast([{{embedding}}] as float[{{embedding_size}}])) as raw_embd_score
    from {{table_name}}
),
fts_candidates as (
    select uuid from scored
    where raw_fts_score is not null
    order by raw_fts_score desc, uuid
    limit {{top_n}}
),
vector_candidates as (
    select uuid from scored
    order by raw_embd_score desc nulls last, uuid
    limit {{top_n}}
),
candidates as (
    select uuid from fts_candidates
    union
    select uuid from vector_candidates
),
normalized_scores as (
    select
        scored.*,
        coalesce(raw_fts_score / nullif(max(raw_fts_score) over (), 0), 0) as norm_fts_score,
        coalesce((raw_embd_score + 1) / nullif(max(raw_embd_score) over () + 1, 0), 0) as norm_embd_score
    from scored inner join candidates using (uuid)
)
select
    uuid,
    chunk,
    path,
    raw_fts_score,
    raw_embd_score,
    norm_fts_score,
    norm_embd_score,
    (0.8 * norm_embd_score + 0.2 * norm_fts_score) as score_cc
from normalized_scores
order by score_cc desc, uuid
limit {{top_k}};
