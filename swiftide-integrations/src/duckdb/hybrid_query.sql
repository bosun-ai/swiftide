with fts as (
    select 
        uuid, 
        chunk, 
        path,
        fts_main_movies.match_bm25(
            uuid,
            ?1,
            fields := chunk
        ) as score
    from {{table_name}}
    limit {{top_n}}
),
embd as (
    select 
        uuid, 
        chunk, 
        path,
        array_cosine_similarity({{embedding_name}}, cast([?2] as float[{{embedding_size}}])) as score
    from {{table_name}}
    limit {{top_n}}
),
normalized_scores as (
    select 
        fts.uuid, 
        fts.chunk, 
        fts.path,
        fts.score as raw_fts_score, 
        embd.score as raw_embd_score,
        (fts.score / (select max(score) from fts)) as norm_fts_score,
        ((embd.score + 1) / (select max(score) + 1 from embd)) as norm_embd_score
    from 
        fts
    inner join
        embd 
    on fts.uuid = embd.uuid
)
select 
    uuid,
    chunk,
    path,
    raw_fts_score, 
    raw_embd_score, 
    norm_fts_score, 
    norm_embd_score, 
    -- (alpha * norm_embd_score + (1-alpha) * norm_fts_score)
    (0.8*norm_embd_score + 0.2*norm_fts_score) AS score_cc
from 
    normalized_scores
order by 
    score_cc desc
limit {{top_k}};
