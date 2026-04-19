using System.Runtime.InteropServices;

namespace TaxGrpc;

[Guid("278E2A10-B2A8-4960-8BE2-7954EFA4E6B0")]
[ClassInterface(ClassInterfaceType.None)]
[ComVisible(true)]
public class Answer : IAnswer
{
	public string Id { get; set; }

	public string IdData { get; set; }

	public string IdSign { get; set; }

	public int Status { get; set; }

	public string Message { get; set; }
}
