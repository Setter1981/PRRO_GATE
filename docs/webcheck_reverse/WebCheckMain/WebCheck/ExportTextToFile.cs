using System;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class ExportTextToFile
{
	private string PF;

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

	public string NewFile()
	{
		string result;
		if (File.Exists(PF))
		{
			try
			{
				File.Delete(PF);
				result = "";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = ex2.Message;
				ProjectData.ClearProjectError();
			}
		}
		else
		{
			result = "";
		}
		return result;
	}

	public string SaveTextToFile(string TextSTR)
	{
		string result;
		try
		{
			StreamWriter streamWriter = new StreamWriter(PF, append: true);
			streamWriter.WriteLine(TextSTR);
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = "";
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = ex2.Message;
			ProjectData.ClearProjectError();
		}
		return result;
	}
}
