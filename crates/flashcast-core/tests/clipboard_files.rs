//! 文件与视频文件剪贴板历史（ticket 12）的宿主级集成测试。
//!
//! 全部经**宿主的查询与命令入口**（`query` / `preview` / `execute` / 管理入口）验证，
//! 穿透真实的 SQLite 文件、真实的临时设备目录与**真实的临时文件**；平台侧用
//! `flashcast_platform::fake` 的替身（替身通过不证明真实平台适配通过——真实文件列表
//! 读写由 `flashcast-platform-check` 在可执行的会话里报告）。
//!
//! v0.1.0 把视频按文件处理，因此下面的文件列表里也包含视频扩展名；不包含录屏、
//! 视频片段提取或任意私有格式的保证。

mod support;

use std::path::{Path, PathBuf};

use flashcast_core::{
    ActionStatus, ClipboardCaptureOutcome, ClipboardCopyError, ClipboardFormat, ClipboardSettings,
    ItemKind, Settings,
};
use support::{cleanup, clipboard_host, fast_settings, focused_app, unique_dir};

/// 默认的剪贴板设置（保留 30 天、容量 500）。
fn settings_with(retention_days: u32, capacity: usize) -> Settings {
    Settings {
        clipboard: ClipboardSettings {
            paused: false,
            retention_days,
            capacity,
        },
        ..fast_settings()
    }
}

/// 造一个真实文件并返回路径。
fn make_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("写入测试文件");
    path
}

/// 多文件列表：含**空格**与**非 ASCII** 名称，还有一个视频文件。
fn multi_file_list(dir: &Path) -> Vec<PathBuf> {
    vec![
        make_file(dir, "报告 草稿.pdf", b"pdf-bytes"),
        make_file(dir, "照片 一.png", b"png-bytes"),
        make_file(dir, "视频 片段.mp4", b"mp4-bytes"),
    ]
}

/// 所有宿主入口都要求先启用插件。
fn enabled_harness(settings: Settings) -> support::ClipboardHarness {
    let harness = clipboard_host(settings);
    harness.enable();
    harness
}

// ---------------------------------------------------------------------------
// 捕获：多个文件、空格、非 ASCII、视频按文件处理
// ---------------------------------------------------------------------------

#[test]
fn capturing_a_multi_file_list_keeps_names_spaces_and_non_ascii() {
    let fixture = unique_dir("clip-files-capture");
    let files = multi_file_list(&fixture);
    let harness = enabled_harness(fast_settings());
    harness.copy_files(&files);

    let event = harness
        .entries()
        .into_iter()
        .next()
        .expect("必须捕获到一条文件历史");
    // 视频文件不比别的文件特殊：仍然是同一条列表里的一个文件。
    match &event.formats[0] {
        ClipboardFormat::Files { count, names } => {
            assert_eq!(*count, 3, "三个文件必须都记下来");
            assert_eq!(
                names,
                &vec![
                    "报告 草稿.pdf".to_string(),
                    "照片 一.png".to_string(),
                    "视频 片段.mp4".to_string(),
                ],
                "名称必须原样保留空格与非 ASCII"
            );
        }
        other => panic!("文件列表的格式必须是 Files：{other:?}"),
    }
    assert!(event.text.is_none(), "文件列表没有可索引文字");
    assert_eq!(event.attachments.len(), 3, "每个文件一个引用附件");
    for (attachment, path) in event.attachments.iter().zip(&files) {
        assert_eq!(
            attachment.kind,
            flashcast_core::AttachmentKind::FileReference,
            "捕获产生的都是原文件引用"
        );
        assert!(attachment.depends_on_source, "引用必须标明依赖原文件");
        assert_eq!(&attachment.path, path, "引用必须指向原路径");
        assert!(attachment.bytes > 0, "存在文件必须观察到大小");
    }
    assert!(
        event.content_hash.starts_with("files:"),
        "文件列表的去重键必须是文件指纹：{}",
        event.content_hash
    );
    assert!(
        event.summary.contains("报告 草稿.pdf"),
        "摘要里必须能按名称看到文件：{}",
        event.summary
    );
    // 尺寸按扩展名推断，视频与图片各归各的。
    let mimes: Vec<Option<&str>> = event
        .attachments
        .iter()
        .map(|attachment| attachment.mime.as_deref())
        .collect();
    assert_eq!(
        mimes,
        vec![
            Some("application/pdf"),
            Some("image/png"),
            Some("video/mp4")
        ]
    );

    cleanup(&fixture);
}

/// 文件列表与文字事件的去重键不互通：同一段路径文字与同一份文件列表是两条历史。
#[test]
fn file_and_text_events_do_not_dedupe_across_types() {
    let fixture = unique_dir("clip-files-dedupe-type");
    let file = make_file(&fixture, "同名.txt", b"x");
    let harness = enabled_harness(fast_settings());
    harness.copy(file.to_string_lossy().as_ref());
    harness.copy_files(&[file.clone()]);

    let entries = harness.entries();
    assert_eq!(entries.len(), 2, "文字与文件列表不能互相去重：{entries:?}");
    let hashes: Vec<&str> = entries
        .iter()
        .map(|event| event.content_hash.as_str())
        .collect();
    assert!(hashes.iter().any(|hash| hash.starts_with("text:")));
    assert!(hashes.iter().any(|hash| hash.starts_with("files:")));

    // 同一份文件列表复制两次才是去重（累加 copies）。
    harness.copy_files(&[file.clone()]);
    let entries = harness.entries();
    assert_eq!(entries.len(), 2, "重复复制同一份列表不新增条目");
    assert_eq!(entries[0].copies, 2, "去重要累加重复次数");

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 列表与预览：引用 vs 已保存副本
// ---------------------------------------------------------------------------

#[test]
fn list_and_preview_distinguish_reference_from_saved_copy() {
    let fixture = unique_dir("clip-files-distinguish");
    let file = make_file(&fixture, "报告 草稿.pdf", b"report");
    let second = make_file(&fixture, "别的.txt", b"other");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(&[file.clone(), second.clone()]);

    let item = harness.first_item("剪贴板");
    let subtitle = item.subtitle.clone().unwrap_or_default();
    assert!(
        subtitle.contains("2 个引用"),
        "列表必须说明这些都是引用：{subtitle}"
    );
    assert!(
        !subtitle.contains("已保存副本"),
        "还没保存副本时不能出现副本标记：{subtitle}"
    );

    let event = harness.entries()[0].clone();
    let attachment_id = event.attachments[0].id.clone();

    // 预览明确说明「引用」与「可恢复」。
    let preview = harness.host.preview(&item.id).expect("预览必须可用");
    let body = preview_body(preview);
    assert!(body.contains("报告 草稿.pdf"), "预览要列出文件：{body}");
    assert!(body.contains("引用"), "预览必须区分引用：{body}");
    assert!(!body.contains("已保存副本"), "还没有副本：{body}");
    assert!(body.contains("可恢复"), "原文件还在时必须可恢复：{body}");

    // 显式为**其中一个**文件保存副本：引用与副本同时出现在同一条历史里。
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &attachment_id)
        .expect("显式保存副本必须成功");
    assert!(!copy.depends_on_source, "副本不依赖原文件");
    assert_eq!(copy.kind, flashcast_core::AttachmentKind::FileCopy);
    assert_eq!(copy.id, attachment_id, "副本是引用行原地改写，标识不变");
    assert!(copy.path.is_file(), "副本必须真的落在本机附件目录里");
    assert!(
        copy.path
            .starts_with(harness.device_dir.join("clipboard").join("attachments")),
        "副本必须落在 <设备目录>/clipboard/attachments/ 下：{}",
        copy.path.display()
    );

    let item = harness.first_item("剪贴板");
    let subtitle = item.subtitle.clone().unwrap_or_default();
    assert!(subtitle.contains("1 个引用"), "{subtitle}");
    assert!(subtitle.contains("1 个已保存副本"), "{subtitle}");
    let preview = harness.host.preview(&item.id).expect("预览必须可用");
    let body = preview_body(preview);
    assert!(body.contains("引用"), "{body}");
    assert!(body.contains("已保存副本"), "{body}");

    // 列表侧（宿主文件条目视图）逐条给出状态。
    let views = harness
        .host
        .clipboard_file_views(&event.id)
        .expect("文件条目状态");
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].kind, flashcast_core::AttachmentKind::FileCopy);
    assert!(views[0].recoverable);
    assert_eq!(views[1].kind, flashcast_core::AttachmentKind::FileReference);
    assert!(views[1].recoverable);

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 原文件删除：引用不可恢复，副本仍可恢复
// ---------------------------------------------------------------------------

#[test]
fn reference_shows_unrecoverable_after_the_original_is_deleted() {
    let fixture = unique_dir("clip-files-missing");
    let file = make_file(&fixture, "会消失.txt", b"gone");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(std::slice::from_ref(&file));
    let item = harness.first_item("剪贴板");

    std::fs::remove_file(&file).expect("删除原文件");

    let preview = harness.host.preview(&item.id).expect("预览必须可用");
    let body = preview_body(preview);
    assert!(
        body.contains("不可恢复"),
        "原文件被删除后引用必须显示不可恢复：{body}"
    );
    assert!(body.contains("会消失.txt"), "预览仍要列出这个文件：{body}");

    // 列表侧（宿主文件条目视图）同样如实报告。
    let event_id = harness.entries()[0].id.clone();
    let views = harness
        .host
        .clipboard_file_views(&event_id)
        .expect("文件条目状态");
    assert_eq!(views.len(), 1);
    assert!(!views[0].recoverable);
    assert!(views[0].problem.as_deref().unwrap_or("").contains("原文件"));

    // 恢复必须如实失败，不能把半份列表放进剪贴板。
    harness.summon(focused_app("file-manager"));
    let outcome = harness.host.execute(&item);
    assert_eq!(
        outcome.status,
        ActionStatus::Failed,
        "引用失效时恢复必须失败：{outcome:?}"
    );
    assert!(
        outcome
            .message
            .as_deref()
            .unwrap_or("")
            .contains("不可恢复"),
        "失败原因必须说清不可恢复：{outcome:?}"
    );
    assert_eq!(
        harness.clipboard.last_write_files(),
        None,
        "不能把缺了文件的列表写进剪贴板"
    );

    cleanup(&fixture);
}

#[test]
fn explicitly_saved_copy_is_still_restorable_after_the_original_is_deleted() {
    let fixture = unique_dir("clip-files-copy-survives");
    let file = make_file(&fixture, "要保住.txt", b"precious-bytes");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(std::slice::from_ref(&file));

    let event = harness.entries()[0].clone();
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("显式保存副本");
    // 原文件从磁盘上消失。
    std::fs::remove_file(&file).expect("删除原文件");
    assert!(!file.exists());

    let item = harness.first_item("剪贴板");
    let preview = harness.host.preview(&item.id).expect("预览必须可用");
    let body = preview_body(preview);
    assert!(
        body.contains("已保存副本") && body.contains("可恢复"),
        "副本必须在原文件删除后仍显示可恢复：{body}"
    );
    assert!(
        !body.contains("不可恢复"),
        "有副本的文件不应显示不可恢复：{body}"
    );

    // 恢复：剪贴板里必须是副本路径，且副本内容与原文件一致。
    harness.summon(focused_app("file-manager"));
    let outcome = harness.host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::PastePending, "{outcome:?}");
    assert_eq!(
        harness.clipboard.last_write_files(),
        Some(vec![copy.path.clone()]),
        "恢复必须使用本机副本"
    );
    let done = harness.host.complete_paste();
    assert_eq!(done.status, ActionStatus::Done);
    assert_eq!(
        harness.paster.files_at_paste(),
        vec![Some(vec![copy.path.clone()])],
        "注入粘贴时剪贴板里必须是这份文件列表"
    );
    assert_eq!(
        std::fs::read(&copy.path).expect("读取副本"),
        b"precious-bytes",
        "副本内容必须与原文件一致"
    );

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 恢复：整份文件列表与复制语义
// ---------------------------------------------------------------------------

#[test]
fn restoring_puts_the_exact_file_list_on_the_clipboard() {
    let fixture = unique_dir("clip-files-restore");
    let files = multi_file_list(&fixture);
    let harness = enabled_harness(fast_settings());
    harness.copy_files(&files);
    let item = harness.first_item("剪贴板");

    harness.summon(focused_app("file-manager"));
    let outcome = harness.host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::PastePending, "{outcome:?}");
    let plan = outcome.paste.clone().expect("必须有粘贴计划");
    assert_eq!(plan.files, 3, "计划里必须说明这是 3 个文件");
    assert_eq!(
        harness.clipboard.last_write_files(),
        Some(files.clone()),
        "剪贴板里必须是**同一份**文件列表（顺序与路径都不变）"
    );
    assert_eq!(
        harness.clipboard.last_write(),
        None,
        "文件列表不是文字，不能同时写成文字"
    );

    let done = harness.host.complete_paste();
    assert_eq!(done.status, ActionStatus::Done);
    assert_eq!(harness.paster.paste_count(), 1);
    assert_eq!(harness.paster.files_at_paste(), vec![Some(files.clone())]);

    // 自身写入抑制：恢复之后立刻再捕获一次，不能把这次恢复收成新历史。
    let before = harness.entries().len();
    let outcome = harness.host.capture_clipboard_once();
    assert!(
        matches!(
            outcome,
            ClipboardCaptureOutcome::Unchanged | ClipboardCaptureOutcome::Suppressed
        ),
        "恢复文件列表不能形成自身捕获循环：{outcome:?}"
    );
    assert_eq!(harness.entries().len(), before, "历史不能多出一条");

    cleanup(&fixture);
}

/// 恢复必须复用 ticket 08 的「复制 + 手动粘贴」回退，而不是假装粘贴成功。
#[test]
fn restore_falls_back_to_manual_paste_when_there_is_no_target() {
    let fixture = unique_dir("clip-files-manual");
    let file = make_file(&fixture, "手动.txt", b"manual");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(std::slice::from_ref(&file));
    let item = harness.first_item("剪贴板");

    harness.host.set_paste_target(None);
    let outcome = harness.host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::CopiedNeedsManualPaste);
    let message = outcome.message.unwrap_or_default();
    assert!(message.contains("手动粘贴"), "应提示手动粘贴：{message}");
    assert_eq!(
        harness.clipboard.last_write_files(),
        Some(vec![file.clone()]),
        "降级路径也必须已经把文件列表放进剪贴板"
    );
    assert_eq!(harness.paster.paste_count(), 0, "不能注入按键");

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 检索：名称与元数据
// ---------------------------------------------------------------------------

#[test]
fn search_finds_a_file_by_name_even_when_the_summary_is_truncated() {
    let fixture = unique_dir("clip-files-search");
    // 造足够多的文件，让摘要一定被截断：最后一个名字只存在于 formats.names 里。
    let mut files = Vec::new();
    for index in 0..12 {
        files.push(make_file(
            &fixture,
            &format!("材料-{index:02}-一段足够长的名称.txt"),
            b"x",
        ));
    }
    let last = make_file(&fixture, "独一无二的尾巴-麒麟.txt", b"x");
    files.push(last);
    let harness = enabled_harness(fast_settings());
    harness.copy_files(&files);

    let event = harness.entries()[0].clone();
    assert!(
        !event.summary.contains("独一无二的尾巴-麒麟"),
        "这个名称必须已经被摘要截断，检索才真的走了文件名称：{}",
        event.summary
    );

    // 经插件范围按名称检索。先输入关键词进入剪贴板范围，再输入名称——这正是用户
    // 的实际操作（spec「输入剪贴板或剪切板搜索历史」）。
    let listed = support::plugin_query(&harness.host, "剪贴板");
    assert_eq!(listed.items.len(), 1, "进入范围后应看到这条历史");
    let response = harness.host.query("独一无二的尾巴-麒麟");
    assert_eq!(
        response.items.len(),
        1,
        "按文件名称必须能找到：{response:?}"
    );
    assert_eq!(response.items[0].kind, ItemKind::ClipboardEntry);

    // 按扩展名（元数据）也能找到。
    let response = harness.host.query("麒麟.txt");
    assert_eq!(response.items.len(), 1, "按文件名片段也要能找到");

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 容量、访问失败、复制中断、不支持的类型
// ---------------------------------------------------------------------------

#[test]
fn entry_capacity_refuses_new_file_lists_with_an_accurate_state() {
    let fixture = unique_dir("clip-files-capacity");
    let first = make_file(&fixture, "第一条.txt", b"1");
    let second = make_file(&fixture, "第二条.txt", b"2");
    let harness = enabled_harness(settings_with(30, 1));
    harness.copy_files(std::slice::from_ref(&first));
    // 置顶之后容量已满且无可回收，必须如实拒绝。
    let event_id = harness.entries()[0].id.clone();
    harness
        .host
        .pin_clipboard_entry(&event_id, true)
        .expect("置顶");

    match harness.copy_files(std::slice::from_ref(&second)) {
        ClipboardCaptureOutcome::CapacityReached { entries, capacity } => {
            assert_eq!((entries, capacity), (1, 1));
        }
        other => panic!("容量已满时必须如实拒绝：{other:?}"),
    }
    let state = harness.host.clipboard_state();
    let reached = state.capacity_reached.expect("状态里必须能看到容量触顶");
    assert!(reached.contains("置顶"), "{reached}");
    assert_eq!(state.entries, 1);

    cleanup(&fixture);
}

#[test]
fn copy_limits_are_reported_with_numbers() {
    let fixture = unique_dir("clip-files-limits");
    let file = make_file(&fixture, "大文件.bin", b"0123456789");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(std::slice::from_ref(&file));
    let event = harness.entries()[0].clone();
    let attachment_id = event.attachments[0].id.clone();

    // 单份超限：如实给出字节数与上限。
    let error = harness
        .host
        .save_clipboard_file_copy_limited(&event.id, &attachment_id, 5, 1024)
        .expect_err("超过单份上限必须被拒绝");
    match error {
        flashcast_core::ClipboardActionError::Copy(ClipboardCopyError::TooLarge {
            bytes,
            limit,
        }) => {
            assert_eq!(bytes, 10);
            assert_eq!(limit, 5);
        }
        other => panic!("必须是「超过单份上限」：{other:?}"),
    }

    // 总量超限：同样给出数字。
    let error = harness
        .host
        .save_clipboard_file_copy_limited(&event.id, &attachment_id, 1024, 5)
        .expect_err("超过总上限必须被拒绝");
    match error {
        flashcast_core::ClipboardActionError::Copy(ClipboardCopyError::TotalLimitReached {
            used,
            limit,
            bytes,
        }) => {
            assert_eq!(used, 0, "还没有任何副本");
            assert_eq!(limit, 5);
            assert_eq!(bytes, 10);
        }
        other => panic!("必须是「超过总容量」：{other:?}"),
    }
    assert!(
        harness.clipboard_store_files().is_empty(),
        "被拒绝的复制不能留下任何文件"
    );

    // 访问失败：原文件消失后如实报告「原文件已不存在」。
    std::fs::remove_file(&file).expect("删除原文件");
    let error = harness
        .host
        .save_clipboard_file_copy(&event.id, &attachment_id)
        .expect_err("原文件不存在时不能假装复制成功");
    assert!(
        matches!(
            error,
            flashcast_core::ClipboardActionError::Copy(ClipboardCopyError::SourceMissing(_))
        ),
        "必须是原文件缺失：{error:?}"
    );

    cleanup(&fixture);
}

#[test]
fn interrupted_copy_and_unsupported_type_are_reported() {
    let fixture = unique_dir("clip-files-failure");
    let file = make_file(&fixture, "中断.bin", b"payload");
    let harness = enabled_harness(fast_settings());
    harness.copy_files(std::slice::from_ref(&file));
    let event = harness.entries()[0].clone();
    let attachment = event.attachments[0].clone();

    // 复制中断：在副本落点上放一个同名目录，原子改名必然失败。
    let store = harness.host.clipboard_store();
    let target = store.file_copy_target_path(&file, attachment.bytes, &attachment.name);
    std::fs::create_dir_all(&target).expect("占位目录");
    let error = harness
        .host
        .save_clipboard_file_copy(&event.id, &attachment.id)
        .expect_err("改名失败必须被报告为复制中断");
    match error {
        flashcast_core::ClipboardActionError::Copy(ClipboardCopyError::CopyFailed { reason }) => {
            assert!(reason.contains("失败"), "原因要可读：{reason}");
        }
        other => panic!("必须是复制中断：{other:?}"),
    }
    assert!(
        !harness.clipboard_store_files().iter().any(|path| path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with(".part-"))
            .unwrap_or(false)),
        "中断的复制不能留下临时文件"
    );
    std::fs::remove_dir(&target).expect("清理占位目录");

    // 不支持的类型：目录不是可复制的文件。
    let directory = make_file(&fixture, "占位.txt", b"x");
    std::fs::remove_file(&directory).expect("删除占位文件");
    std::fs::create_dir(&directory).expect("建目录");
    harness.copy_files(std::slice::from_ref(&directory));
    let event = harness
        .entries()
        .into_iter()
        .find(|event| event.attachments[0].path == directory)
        .expect("目录也算一次捕获");
    let error = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect_err("目录必须被拒绝");
    assert!(
        matches!(
            error,
            flashcast_core::ClipboardActionError::Copy(ClipboardCopyError::UnsupportedType(_))
        ),
        "必须是不支持的类型：{error:?}"
    );
    // 目录引用在预览里也要如实显示。
    let view = harness
        .host
        .clipboard_file_views(&event.id)
        .expect("文件条目状态");
    assert!(!view[0].recoverable);
    assert!(
        view[0]
            .problem
            .as_deref()
            .unwrap_or("")
            .contains("不支持的文件类型"),
        "{:?}",
        view[0].problem
    );

    cleanup(&fixture);
}

// ---------------------------------------------------------------------------
// 生命周期：回收与共享附件
// ---------------------------------------------------------------------------

#[test]
fn delete_clear_and_expiry_reclaim_unreferenced_attachments() {
    let fixture = unique_dir("clip-files-reclaim");
    let first = make_file(&fixture, "一.txt", b"one");
    let second = make_file(&fixture, "二.txt", b"two");
    let harness = enabled_harness(settings_with(1, 500));

    // 删除一条：它的副本文件被回收。
    harness.copy_files(std::slice::from_ref(&first));
    let event = harness.entries()[0].clone();
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("保存副本");
    assert!(copy.path.is_file());
    harness
        .host
        .delete_clipboard_entry(&event.id)
        .expect("删除条目");
    assert!(
        !copy.path.exists(),
        "删除历史必须回收不再被引用的副本：{}",
        copy.path.display()
    );

    // 过期：保留 1 天，把「现在」推到两天后回收一次（宿主在保存与改设置时都走这条
    // 回收路径，这里直接驱动同一个公开入口，不伪造时间以外的任何东西）。
    harness.copy_files(std::slice::from_ref(&second));
    let event = harness.entries()[0].clone();
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("保存副本");
    assert!(copy.path.is_file());
    harness
        .host
        .set_clipboard_limits(1, 500)
        .expect("改保留期限");
    let future = flashcast_core::now_ms() + 2 * 86_400_000;
    harness
        .host
        .clipboard_store()
        .reclaim(1, 500, future)
        .expect("过期回收");
    assert!(
        !copy.path.exists(),
        "过期回收必须同步回收副本：{}",
        copy.path.display()
    );
    assert!(harness.entries().is_empty());

    // 清空：同样回收。
    harness.copy_files(std::slice::from_ref(&first));
    let event = harness.entries()[0].clone();
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("保存副本");
    assert!(copy.path.is_file());
    harness.host.clear_clipboard_history().expect("清空");
    assert!(
        !copy.path.exists(),
        "清空历史必须回收副本：{}",
        copy.path.display()
    );
    assert!(harness.clipboard_store_files().is_empty());

    cleanup(&fixture);
}

/// **去重不能误删共享附件**（spec 明确点名的行为）。
///
/// 两个不同的文件列表都引用了同一个原文件，各自显式保存副本——副本落在同一个文件上。
/// 删掉其中一条历史之后，另一条必须仍然可以恢复，磁盘上那份副本也不能被删掉。
#[test]
fn deleting_one_entry_keeps_a_shared_attachment_for_the_other() {
    let fixture = unique_dir("clip-files-shared");
    let shared = make_file(&fixture, "共享.bin", b"shared-bytes");
    let other = make_file(&fixture, "别的.txt", b"other");
    let harness = enabled_harness(fast_settings());

    // 列表 A：[共享]；列表 B：[共享, 别的] —— 内容不同，因此是两条历史。
    harness.copy_files(std::slice::from_ref(&shared));
    let event_a = harness.entries()[0].clone();
    harness.copy_files(&[shared.clone(), other.clone()]);
    let event_b = harness
        .entries()
        .into_iter()
        .find(|event| event.id != event_a.id)
        .expect("第二条文件历史");
    assert_eq!(event_a.attachments.len(), 1);
    assert_eq!(event_b.attachments.len(), 2);

    let copy_a = harness
        .host
        .save_clipboard_file_copy(&event_a.id, &event_a.attachments[0].id)
        .expect("A 保存副本");
    let copy_b = harness
        .host
        .save_clipboard_file_copy(&event_b.id, &event_b.attachments[0].id)
        .expect("B 保存副本");
    assert_eq!(
        copy_a.path, copy_b.path,
        "同一个原文件的副本必须复用同一个文件（去重）"
    );

    // 原文件删掉，才能证明「可恢复」靠的是那份共享副本。
    std::fs::remove_file(&shared).expect("删除原文件");

    harness
        .host
        .delete_clipboard_entry(&event_a.id)
        .expect("删除 A");
    assert!(
        copy_b.path.is_file(),
        "删除 A 不能把 B 还在用的共享副本删掉：{}",
        copy_b.path.display()
    );

    // B 仍然可恢复：第一个文件是共享副本（可恢复），第二个文件仍是引用。
    let view = harness
        .host
        .clipboard_file_views(&event_b.id)
        .expect("B 的文件条目状态");
    assert_eq!(
        view[0].kind,
        flashcast_core::AttachmentKind::FileCopy,
        "B 的第一个文件必须是共享副本：{view:?}"
    );
    assert!(view[0].recoverable, "B 的共享文件必须仍可恢复：{view:?}");
    assert_eq!(view[0].path, copy_b.path, "恢复用的是共享副本路径");

    let item = support::plugin_query(&harness.host, "剪贴板")
        .items
        .into_iter()
        .find(|item| item.id.ends_with(&event_b.id))
        .expect("B 的条目");
    harness.summon(focused_app("file-manager"));
    let outcome = harness.host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::PastePending, "{outcome:?}");
    let written = harness
        .clipboard
        .last_write_files()
        .expect("必须写出文件列表");
    assert_eq!(written.len(), 2, "B 的列表必须完整");
    assert_eq!(
        written[0], copy_b.path,
        "原文件已删除，第一个文件必须用共享副本"
    );
    assert_eq!(written[1], other, "第二个文件仍然是原路径");

    cleanup(&fixture);
}

/// **原文件永远不会被移动或删除**：保存副本、恢复、删除历史、清空、过期回收之后，
/// 原文件都必须在原位置、内容不变，副本是另一个文件。
#[test]
fn the_original_file_is_never_moved_or_deleted() {
    let fixture = unique_dir("clip-files-original");
    let original = make_file(&fixture, "原件.bin", b"original-bytes");
    let harness = enabled_harness(settings_with(1, 500));
    harness.copy_files(std::slice::from_ref(&original));
    let event = harness.entries()[0].clone();

    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("保存副本");
    assert_eq!(
        std::fs::read(&original).expect("原文件必须还在"),
        b"original-bytes"
    );
    assert_ne!(copy.path, original, "副本必须是另一个文件");

    // 恢复一次：同样不碰原文件。
    let item = harness.first_item("剪贴板");
    harness.summon(focused_app("file-manager"));
    harness.host.execute(&item);

    // 删除条目、过期回收、清空：都只回收副本。
    harness
        .host
        .delete_clipboard_entry(&event.id)
        .expect("删除");
    assert_eq!(
        std::fs::read(&original).expect("删除历史后原文件必须还在"),
        b"original-bytes"
    );
    assert!(!copy.path.exists(), "副本被回收");

    // 清空：同样只回收副本，原文件不动。
    // （用第二个文件：刚刚恢复过第一个文件列表，宿主会把它登记为自身写入并抑制一次，
    // 这是防止自身捕获循环的正确行为。）
    let second = make_file(&fixture, "原件二.bin", b"second-bytes");
    harness.copy_files(std::slice::from_ref(&second));
    let event = harness.entries()[0].clone();
    let copy = harness
        .host
        .save_clipboard_file_copy(&event.id, &event.attachments[0].id)
        .expect("再保存一次副本");
    harness.host.clear_clipboard_history().expect("清空");
    assert_eq!(
        std::fs::read(&original).expect("清空后原文件必须还在"),
        b"original-bytes"
    );
    assert_eq!(
        std::fs::read(&second).expect("第二个原文件也必须还在"),
        b"second-bytes"
    );
    assert!(!copy.path.exists());

    cleanup(&fixture);
}

fn preview_body(preview: flashcast_core::Preview) -> String {
    match preview {
        flashcast_core::Preview::Text { body, .. } => body,
        other => panic!("文件历史的预览应是文本：{other:?}"),
    }
}

/// 测试用的扩展：直接看附件目录里有哪些文件。
trait ClipboardHarnessFiles {
    fn clipboard_store_files(&self) -> Vec<PathBuf>;
}

impl ClipboardHarnessFiles for support::ClipboardHarness {
    fn clipboard_store_files(&self) -> Vec<PathBuf> {
        let dir = self.device_dir.join("clipboard").join("attachments");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file())
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
    }
}
