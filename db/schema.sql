-- gomachine 표준 스키마 (MariaDB, utf8mb4).
-- 이름 규칙: 테이블 {name}_tb, 컬럼 {접두사}_{이름}. 접두사는 테이블마다 하나 (user_tb → u_, board_tb → b_).
-- {접두사}_createddate 는 생성기가 Insert 때 현재 시각을 넣는다 (DB 기본값도 현재 시각).
-- docker-compose 의 db 서비스가 처음 뜰 때 /docker-entrypoint-initdb.d 에서 한 번 실행한다.

CREATE TABLE IF NOT EXISTS `user_tb` (
  `u_id` bigint NOT NULL AUTO_INCREMENT,
  `u_email` varchar(255) NOT NULL,
  `u_passwd` varchar(255) NOT NULL,
  `u_name` varchar(100) NOT NULL,
  `u_level` int NOT NULL DEFAULT 1,
  `u_createddate` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`u_id`),
  UNIQUE KEY `uk_user_email` (`u_email`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS `board_tb` (
  `b_id` bigint NOT NULL AUTO_INCREMENT,
  `b_title` varchar(200) NOT NULL,
  `b_content` text,
  `b_img` varchar(255) DEFAULT NULL,
  `b_user` bigint NOT NULL,
  `b_createddate` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`b_id`),
  KEY `idx_board_user` (`b_user`),
  CONSTRAINT `fk_board_user` FOREIGN KEY (`b_user`) REFERENCES `user_tb` (`u_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
