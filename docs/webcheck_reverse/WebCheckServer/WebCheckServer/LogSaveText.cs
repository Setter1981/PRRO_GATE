using System;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheckServer;

internal class LogSaveText
{
	private string PF;

	public bool LogOn;

	public string PathFile
	{
		get
		{
			return PF;
		}
		set
		{
			PF = value;
		}
	}

	public LogSaveText()
	{
		LogOn = true;
	}

	public void SaveTextToLog(string FN, string NameSLog, string TextIn, string TextOut = "")
	{
		string value = DateTime.Now.ToString();
		NameSLog = FN + "   " + NameSLog + "    SERVER v.1.3.5";
		try
		{
			StreamWriter streamWriter = new StreamWriter(PF, append: true);
			streamWriter.WriteLine(value);
			streamWriter.WriteLine(NameSLog);
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(TextIn);
			streamWriter.WriteLine("  ");
			if (Operators.CompareString(TextOut.Trim(), "", false) != 0)
			{
				streamWriter.WriteLine(TextOut);
			}
			streamWriter.WriteLine("  ");
			streamWriter.WriteLine(" =====================================================");
			streamWriter.WriteLine("  ");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
