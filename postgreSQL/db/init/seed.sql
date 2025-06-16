-- DROP VIEW IF EXISTS author_stats, album_stats, track_stats CASCADE;
-- DROP PROCEDURE IF EXISTS add_track(INT, TEXT, INT, TEXT);
-- DROP PROCEDURE IF EXISTS record_listen(INT);
-- DROP TABLE IF EXISTS listenings, users, track_albums, tracks, authors CASCADE;
-- DROP TYPE IF EXISTS track_input, album_input, author_input CASCADE;


-- =================================================================
-- 1. SCHEMAS 
-- =================================================================

CREATE TABLE authors (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  img       TEXT
);

CREATE TABLE tracks (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  duration  INT     NOT NULL, -- Duration in ms
  img       TEXT,
  author_id INT     NOT NULL REFERENCES authors(id) ON DELETE CASCADE
);

CREATE TABLE listenings (
  id          BIGSERIAL PRIMARY KEY ,
  track_id    INT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  listened_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON listenings (track_id);


-- =================================================================
-- 2. TYPES (Unchanged)
-- =================================================================

CREATE TYPE author_input AS (
  id        INT,
  title     TEXT,
  img       TEXT
);

CREATE TYPE album_input AS (
  id        INT,
  title     TEXT,
  img       TEXT,
  author_id INT
);

CREATE TYPE track_input AS (
  id        INT,
  title     TEXT,
  duration  INT,
  img       TEXT
);


-- =================================================================
-- 3. PROCEDURES
-- =================================================================



CREATE OR REPLACE PROCEDURE add_track(
  _track      track_input,
  _author     author_input
)
LANGUAGE plpgsql
AS $$
BEGIN
INSERT INTO authors (id, title, img)
VALUES (_author.id, _author.title, _author.img)
    ON CONFLICT (id) DO NOTHING;

INSERT INTO tracks (id, title, duration, img, author_id)
VALUES (_track.id, _track.title, _track.duration, NULLIF(_track.img, ''), _author.id)
    ON CONFLICT (id) DO NOTHING;
END;
$$;


CREATE OR REPLACE FUNCTION record_listen(
  _track_id INT
)
RETURNS BOOLEAN AS $$
BEGIN
  -- Пытаемся вставить запись
INSERT INTO listenings (track_id) VALUES (_track_id);

-- Если мы дошли до этой строки, значит INSERT прошел успешно
RETURN TRUE;

EXCEPTION
  -- Перехватываем только ошибку нарушения внешнего ключа
  WHEN foreign_key_violation THEN
    -- Эта ошибка означает, что трека с _track_id не существует
    -- Возвращаем FALSE вместо того, чтобы позволить функции "упасть"
    RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- =================================================================
-- 4. LISTENING STATS - VIEWS
-- =================================================================

CREATE OR REPLACE VIEW track_stats AS
SELECT
  t.id AS track_id,
  t.title,
  t.duration,
  COUNT(l.id) AS listen_count
FROM tracks t
LEFT JOIN listenings l ON t.id = l.track_id
GROUP BY t.id;

