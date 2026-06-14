-- poshgre test environment: schema definitions
-- All identifiers and data are in English to avoid character encoding issues.
-- This script runs connected to testdb (POSTGRES_DB).

-- ============================================================
-- public schema: users, products, orders (default search_path)
-- ============================================================

CREATE TABLE IF NOT EXISTS users (
    id          SERIAL          NOT NULL,
    username    VARCHAR(64)     NOT NULL,
    email       VARCHAR(128)    NOT NULL,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE (email)
);

CREATE TABLE IF NOT EXISTS products (
    id          SERIAL          NOT NULL,
    name        VARCHAR(128)    NOT NULL,
    price       NUMERIC(10,2)   NOT NULL,
    category    VARCHAR(64)     NOT NULL,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS orders (
    id          SERIAL          NOT NULL,
    user_id     INTEGER         NOT NULL,
    product_id  INTEGER         NOT NULL,
    quantity    INTEGER         NOT NULL DEFAULT 1,
    total       NUMERIC(12,2)   NOT NULL,
    ordered_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_orders_user_id    ON orders (user_id);
CREATE INDEX IF NOT EXISTS idx_orders_product_id ON orders (product_id);

-- ============================================================
-- ecommerce schema
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ecommerce;

CREATE TABLE IF NOT EXISTS ecommerce.customers (
    id              SERIAL          NOT NULL,
    first_name      VARCHAR(64)     NOT NULL,
    last_name       VARCHAR(64)     NOT NULL,
    email           VARCHAR(128)    NOT NULL,
    country         VARCHAR(64)     NOT NULL DEFAULT 'US',
    registered_at   TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE (email)
);

CREATE TABLE IF NOT EXISTS ecommerce.categories (
    id          SERIAL          NOT NULL,
    name        VARCHAR(64)     NOT NULL,
    parent_id   INTEGER         NULL,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_categories_parent ON ecommerce.categories (parent_id);

CREATE TABLE IF NOT EXISTS ecommerce.items (
    id          SERIAL          NOT NULL,
    name        VARCHAR(128)    NOT NULL,
    description TEXT,
    price       NUMERIC(10,2)   NOT NULL,
    category_id INTEGER         NOT NULL,
    stock       INTEGER         NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_items_category ON ecommerce.items (category_id);

CREATE TABLE IF NOT EXISTS ecommerce.transactions (
    id              SERIAL          NOT NULL,
    customer_id     INTEGER         NOT NULL,
    item_id         INTEGER         NOT NULL,
    quantity        INTEGER         NOT NULL DEFAULT 1,
    amount          NUMERIC(12,2)   NOT NULL,
    purchased_at    TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_transactions_customer ON ecommerce.transactions (customer_id);
CREATE INDEX IF NOT EXISTS idx_transactions_item     ON ecommerce.transactions (item_id);

CREATE TABLE IF NOT EXISTS ecommerce.reviews (
    id          SERIAL          NOT NULL,
    customer_id INTEGER         NOT NULL,
    item_id     INTEGER         NOT NULL,
    rating      SMALLINT        NOT NULL DEFAULT 3,
    comment     TEXT,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_reviews_customer ON ecommerce.reviews (customer_id);
CREATE INDEX IF NOT EXISTS idx_reviews_item     ON ecommerce.reviews (item_id);

-- ============================================================
-- blog schema
-- ============================================================

CREATE SCHEMA IF NOT EXISTS blog;

CREATE TABLE IF NOT EXISTS blog.authors (
    id      SERIAL          NOT NULL,
    name    VARCHAR(128)    NOT NULL,
    bio     TEXT,
    email   VARCHAR(128)    NOT NULL,
    PRIMARY KEY (id),
    UNIQUE (email)
);

CREATE TABLE IF NOT EXISTS blog.posts (
    id              SERIAL          NOT NULL,
    author_id       INTEGER         NOT NULL,
    title           VARCHAR(256)    NOT NULL,
    body            TEXT,
    status          VARCHAR(16)     NOT NULL DEFAULT 'draft'
                        CHECK (status IN ('draft','published','archived')),
    published_at    TIMESTAMP       NULL,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_posts_author ON blog.posts (author_id);
CREATE INDEX IF NOT EXISTS idx_posts_status ON blog.posts (status);

CREATE TABLE IF NOT EXISTS blog.comments (
    id          SERIAL          NOT NULL,
    post_id     INTEGER         NOT NULL,
    author_name VARCHAR(128)    NOT NULL,
    body        TEXT            NOT NULL,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_comments_post ON blog.comments (post_id);

CREATE TABLE IF NOT EXISTS blog.tags (
    id      SERIAL          NOT NULL,
    name    VARCHAR(64)     NOT NULL,
    PRIMARY KEY (id),
    UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS blog.post_tags (
    post_id INTEGER NOT NULL,
    tag_id  INTEGER NOT NULL,
    PRIMARY KEY (post_id, tag_id)
);

-- ============================================================
-- analytics schema
-- ============================================================

CREATE SCHEMA IF NOT EXISTS analytics;

-- events: large table for streaming/pagination demo (~1M rows)
CREATE TABLE IF NOT EXISTS analytics.events (
    id          BIGSERIAL       NOT NULL,
    event_type  VARCHAR(64)     NOT NULL,
    user_id     INTEGER         NOT NULL,
    page_url    VARCHAR(256)    NOT NULL,
    referrer    VARCHAR(256)    NULL,
    ip_address  VARCHAR(45)     NOT NULL,
    user_agent  VARCHAR(256)    NOT NULL,
    created_at  TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_events_event_type ON analytics.events (event_type);
CREATE INDEX IF NOT EXISTS idx_events_user_id    ON analytics.events (user_id);
CREATE INDEX IF NOT EXISTS idx_events_created_at ON analytics.events (created_at);

CREATE TABLE IF NOT EXISTS analytics.sessions (
    id          BIGSERIAL       NOT NULL,
    user_id     INTEGER         NOT NULL,
    started_at  TIMESTAMP       NOT NULL,
    ended_at    TIMESTAMP       NULL,
    page_count  INTEGER         NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON analytics.sessions (user_id);

CREATE TABLE IF NOT EXISTS analytics.page_views (
    id                  BIGSERIAL   NOT NULL,
    session_id          BIGINT      NOT NULL,
    url                 VARCHAR(256) NOT NULL,
    duration_seconds    INTEGER     NOT NULL DEFAULT 0,
    created_at          TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_page_views_session ON analytics.page_views (session_id);

-- ============================================================
-- inventory schema
-- ============================================================

CREATE SCHEMA IF NOT EXISTS inventory;

CREATE TABLE IF NOT EXISTS inventory.warehouses (
    id          SERIAL          NOT NULL,
    name        VARCHAR(128)    NOT NULL,
    location    VARCHAR(128)    NOT NULL,
    capacity    INTEGER         NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS inventory.products (
    id          SERIAL          NOT NULL,
    sku         VARCHAR(64)     NOT NULL,
    name        VARCHAR(128)    NOT NULL,
    description TEXT,
    unit_price  NUMERIC(10,2)   NOT NULL,
    PRIMARY KEY (id),
    UNIQUE (sku)
);

CREATE TABLE IF NOT EXISTS inventory.stock_levels (
    id              SERIAL      NOT NULL,
    warehouse_id    INTEGER     NOT NULL,
    product_id      INTEGER     NOT NULL,
    quantity        INTEGER     NOT NULL DEFAULT 0,
    last_updated    TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE (warehouse_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_stock_product ON inventory.stock_levels (product_id);

CREATE TABLE IF NOT EXISTS inventory.shipments (
    id              SERIAL          NOT NULL,
    warehouse_id    INTEGER         NOT NULL,
    product_id      INTEGER         NOT NULL,
    quantity        INTEGER         NOT NULL,
    shipped_at      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    destination     VARCHAR(128)    NOT NULL,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_shipments_warehouse ON inventory.shipments (warehouse_id);
CREATE INDEX IF NOT EXISTS idx_shipments_product   ON inventory.shipments (product_id);

-- ============================================================
-- hr schema (abbreviated from hr_system for convenience)
-- ============================================================

CREATE SCHEMA IF NOT EXISTS hr;

CREATE TABLE IF NOT EXISTS hr.departments (
    id          SERIAL          NOT NULL,
    name        VARCHAR(128)    NOT NULL,
    -- manager_id references employees; populated after employees are inserted
    manager_id  INTEGER         NULL,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS hr.employees (
    id              SERIAL          NOT NULL,
    first_name      VARCHAR(64)     NOT NULL,
    last_name       VARCHAR(64)     NOT NULL,
    email           VARCHAR(128)    NOT NULL,
    department_id   INTEGER         NOT NULL,
    position        VARCHAR(128)    NOT NULL,
    salary          NUMERIC(12,2)   NOT NULL,
    hired_at        DATE            NOT NULL,
    PRIMARY KEY (id),
    UNIQUE (email)
);

CREATE INDEX IF NOT EXISTS idx_employees_department ON hr.employees (department_id);

CREATE TABLE IF NOT EXISTS hr.projects (
    id              SERIAL          NOT NULL,
    name            VARCHAR(128)    NOT NULL,
    department_id   INTEGER         NOT NULL,
    budget          NUMERIC(14,2)   NOT NULL,
    started_at      DATE            NOT NULL,
    deadline        DATE            NOT NULL,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_projects_department ON hr.projects (department_id);

CREATE TABLE IF NOT EXISTS hr.time_entries (
    id              SERIAL          NOT NULL,
    employee_id     INTEGER         NOT NULL,
    project_id      INTEGER         NOT NULL,
    hours           NUMERIC(5,2)    NOT NULL,
    work_date       DATE            NOT NULL,
    description     VARCHAR(256)    NULL,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_time_entries_employee ON hr.time_entries (employee_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_project  ON hr.time_entries (project_id);
