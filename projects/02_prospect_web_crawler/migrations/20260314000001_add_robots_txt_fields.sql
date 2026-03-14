ALTER TYPE crawl_status ADD VALUE 'blocked';

ALTER TABLE domain_metrics 
ADD COLUMN robots_txt_content TEXT,
ADD COLUMN robots_txt_fetched_at TIMESTAMPTZ,
ADD COLUMN robots_txt_status INT;
