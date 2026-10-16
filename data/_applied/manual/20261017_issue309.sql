-- #309: 765as_destiny の release_date は別曲 (VOCAL MASTER の destiny) の 2008-04-02 だった。
-- ワンフォーオールの Destiny の初出日は公式で確認できないので空 (不明) にする。
BEGIN;
UPDATE songs SET release_date = NULL WHERE id = '765as_destiny';
COMMIT;
