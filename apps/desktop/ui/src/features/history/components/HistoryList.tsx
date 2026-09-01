/**
 * 详情通用分节：标题 + 条目列表。空列表不渲染。
 */
export function HistoryList({
    title,
    items,
    muted = false,
}: {
    title: string;
    items: string[];
    muted?: boolean;
}) {
    if (!items.length) return null;
    return (
        <section className={`history-detail-section${muted ? " subdued" : ""}`}>
            <h2>{title}</h2>
            <ul>
                {items.map((item, index) => (
                    <li key={index}>{item}</li>
                ))}
            </ul>
        </section>
    );
}
