using System.Runtime.CompilerServices;

namespace WebCheckServer;

internal class V7Data
{
	private static object m_V7Object;

	private static IErrorLog m_ErrorInfo;

	private static IAsyncEvent m_AsyncEvent;

	private static IStatusLine m_StatusLine;

	public static object V7Object
	{
		get
		{
			return m_V7Object;
		}
		set
		{
			m_V7Object = RuntimeHelpers.GetObjectValue(value);
			m_ErrorInfo = (IErrorLog)value;
			m_AsyncEvent = (IAsyncEvent)value;
			m_StatusLine = (IStatusLine)value;
		}
	}

	public static IErrorLog ErrorLog => m_ErrorInfo;

	public static IAsyncEvent AsyncEvent => m_AsyncEvent;

	public static IStatusLine StatusLine => m_StatusLine;
}
