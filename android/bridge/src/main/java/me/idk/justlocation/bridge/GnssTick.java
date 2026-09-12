package me.idk.justlocation.bridge;

/**
 * 卫星侧各种监听包装的共同面：**该不该由我们投递**、**投递一次**。
 *
 * <p>存在的理由是 `GnssDispatcher` 只认这一个接口——它把"平台的活跃投递"和"具体是哪条
 * GNSS 通道"解耦，卫星状态、NMEA、原始测量、导航电文四条出口共用同一套投递机制。
 */
interface GnssTick {
    /** 当前是否由模拟接管（开关开着、快照新鲜、本应用在作用范围内）。 */
    boolean needsTick();

    /** 按当前快照投递一轮；投递过程中会逐条重查，停止后立刻不再投。 */
    void tick() throws Throwable;

    /**
     * 诊断用：这个包装被进入投递的次数、真正调用到应用监听器的次数。
     *
     * <p>为什么要有这两个数：真机上出现过"投递=0、失败=0"，光看上层计数分不清是
     * **投递没进来**还是**进来了但一条都没送出去**。这两条分别对应两种完全不同的毛病。
     */
    default long ticks() { return -1; }

    default long deliveries() { return -1; }

    /** 诊断用：这条通道的接收方身份与事件方法归属。没有实现就返回 null。 */
    default String description() { return null; }
}
