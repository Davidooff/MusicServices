-- DROP VIEW IF EXISTS author_stats, album_stats, track_stats CASCADE;
-- DROP PROCEDURE IF EXISTS add_track(INT, TEXT, INT, TEXT);
-- DROP PROCEDURE IF EXISTS record_listen(INT);
-- DROP TABLE IF EXISTS listenings, users, track_albums, tracks, authors CASCADE;
-- DROP TYPE IF EXISTS track_input, album_input, author_input CASCADE;


-- =================================================================
-- 1. SCHEMAS 
-- =================================================================

CREATE TABLE authors_soundcloud (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  img       TEXT
);

CREATE TABLE tracks_soundcloud (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  duration  INT     NOT NULL, -- Duration in ms
  img       TEXT,
  author_id INT     NOT NULL REFERENCES authors_soundcloud(id) ON DELETE CASCADE
);

CREATE TABLE listenings_soundcloud (
  id          BIGSERIAL PRIMARY KEY ,
  track_id    INT NOT NULL REFERENCES tracks_soundcloud(id) ON DELETE CASCADE,
  listened_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON listenings_soundcloud (track_id);


-- =================================================================
-- 2. TYPES 
-- =================================================================

CREATE TYPE author_input_soundcloud AS (
  id        INT,
  title     TEXT,
  img       TEXT
);

CREATE TYPE track_input_soundcloud AS (
  id        INT,
  title     TEXT,
  duration  INT,
  img       TEXT
);


-- =================================================================
-- 3. PROCEDURES
-- =================================================================



CREATE OR REPLACE PROCEDURE add_track_soundcloud(
  _track      track_input_soundcloud,
  _author     author_input_soundcloud
)
LANGUAGE plpgsql
AS $$
BEGIN
INSERT INTO authors_soundcloud (id, title, img)
VALUES (_author.id, _author.title, _author.img)
    ON CONFLICT (id) DO NOTHING;

INSERT INTO tracks_soundcloud (id, title, duration, img, author_id)
VALUES (_track.id, _track.title, _track.duration, NULLIF(_track.img, ''), _author.id)
    ON CONFLICT (id) DO NOTHING;
END;
$$;


CREATE OR REPLACE FUNCTION record_listen_soundcloud(
  _track_id INT
)
RETURNS BOOLEAN AS $$
BEGIN
  -- Пытаемся вставить запись
INSERT INTO listenings_soundcloud (track_id) VALUES (_track_id);

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

CREATE OR REPLACE VIEW track_stats_soundcloud AS
SELECT
  t.id AS track_id,
  t.title,
  t.duration,
  COUNT(l.id) AS listen_count
FROM tracks_soundcloud t
LEFT JOIN listenings_soundcloud l ON t.id = l.track_id
GROUP BY t.id;









-- =================================================================
-- 1. SCHEMAS 
-- =================================================================

CREATE TABLE authors_deezer (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  img       TEXT
);

CREATE TABLE albums_deezer (
  id        INT    PRIMARY KEY,
  title     TEXT    NOT NULL,
  img       TEXT,
  author_id INT     NOT NULL REFERENCES authors_deezer(id) ON DELETE CASCADE
);

CREATE TABLE tracks_deezer (
  id        INT     PRIMARY KEY,
  title     TEXT    NOT NULL,
  duration  INT     NOT NULL, -- Duration in ms
  img       TEXT,
  author_id INT     NOT NULL REFERENCES authors_deezer(id) ON DELETE CASCADE,
  album_id  INT     NOT NULL REFERENCES albums_deezer(id) ON DELETE CASCADE
);

CREATE TABLE listenings_deezer (
  id          BIGSERIAL PRIMARY KEY ,
  track_id    INT NOT NULL REFERENCES tracks_deezer(id) ON DELETE CASCADE,
  listened_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON listenings_deezer (track_id);


-- =================================================================
-- 2. TYPES 
-- =================================================================

CREATE TYPE author_input_deezer AS (
  id        INT,
  title     TEXT,
  img       TEXT
);

CREATE TYPE track_input_deezer AS (
  id        INT,
  title     TEXT,
  duration  INT
);

CREATE TYPE album_input_deezer AS (
  id        INT,
  title     TEXT,
  img       TEXT
);


-- =================================================================
-- 3. PROCEDURES
-- =================================================================



CREATE OR REPLACE PROCEDURE add_album_deezer(
    p_author author_input_deezer,
    p_album  album_input_deezer,
    p_tracks track_input_deezer[]
)
LANGUAGE plpgsql
AS $$
DECLARE
    -- Variable to hold each track during the loop
    v_track track_input_deezer;
BEGIN
    -- Step 1: Insert or update the author.
    -- If an author with the same ID already exists, their title and image will be updated.
    -- Otherwise, a new author is created.
    INSERT INTO authors_deezer (id, title, img)
    VALUES (p_author.id, p_author.title, p_author.img)
    ON CONFLICT (id) DO UPDATE SET
      title = EXCLUDED.title,
      img = EXCLUDED.img;

    -- Step 2: Insert or update the album.
    -- This links the album to the author's ID.
    -- If the album already exists, its details are updated.
    INSERT INTO albums_deezer (id, title, img, author_id)
    VALUES (p_album.id, p_album.title, p_album.img, p_author.id)
    ON CONFLICT (id) DO UPDATE SET
      title = EXCLUDED.title,
      img = EXCLUDED.img,
      author_id = EXCLUDED.author_id;

    -- Step 3: Loop through the array of tracks and insert or update each one.
    -- The track's image is assumed to be the same as the album's image, a common pattern.
    FOREACH v_track IN ARRAY p_tracks
    LOOP
      INSERT INTO tracks_deezer (id, title, duration, img, author_id, album_id)
      VALUES (
        v_track.id,
        v_track.title,
        v_track.duration,
        p_album.img, -- Using album's image for the track
        p_author.id,
        p_album.id
      )
      ON CONFLICT (id) DO UPDATE SET
        title = EXCLUDED.title,
        duration = EXCLUDED.duration,
        img = EXCLUDED.img,
        author_id = EXCLUDED.author_id,
        album_id = EXCLUDED.album_id;
    END LOOP;

END;
$$;


CREATE OR REPLACE FUNCTION record_listen_deezer(
  _track_id INT
)
RETURNS BOOLEAN AS $$
BEGIN
  -- Пытаемся вставить запись
INSERT INTO listenings_deezer (track_id) VALUES (_track_id);

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

CREATE OR REPLACE VIEW track_stats_deezer AS
SELECT
  t.id AS track_id,
  t.title,
  t.duration,
  COUNT(l.id) AS listen_count
FROM tracks_deezer t
LEFT JOIN listenings_deezer l ON t.id = l.track_id
GROUP BY t.id;

