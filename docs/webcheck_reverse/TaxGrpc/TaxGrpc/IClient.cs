using System.Runtime.InteropServices;

namespace TaxGrpc;

[Guid("32A8E7F2-3A7A-4970-AF5B-4BE457115525")]
[ComVisible(true)]
public interface IClient
{
	void Open(string host);

	Answer Check(int version, string pathf, string fn, long date, int number, byte type, string idCancel, string idLocal, int timeoutMillis);

	Answer Ping(string pathf, string fn, long date, int number, byte type, string idCancel, string idLocal, int timeoutMillis);

	Answer CheckLast(string pathf, int timeoutMillis);
}
