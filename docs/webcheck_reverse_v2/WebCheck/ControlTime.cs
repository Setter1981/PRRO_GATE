using System;
using System.IO;
using System.Net;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

internal class ControlTime
{
	internal bool TimeContol(int MinutesT = 0)
	{
		if (!All.A.Status)
		{
			return false;
		}
		if (MinutesT < 1)
		{
			return true;
		}
		string text = TimeZ().Trim();
		All.Lg.SaveTextToLog("TimeContol", text);
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			All.Lg.SaveTextToLog("TimeContol", "Внимание!", "Сверка системного времени с сайтом налоговой не проведена.");
			return true;
		}
		DateTime now = DateTime.Now;
		DateTime date = StrToDt(text);
		if (Math.Abs(DateAndTime.DateDiff(DateInterval.Minute, now, date)) > MinutesT)
		{
			return false;
		}
		return true;
	}

	private DateTime StrToDt(string S)
	{
		string text = "";
		DateTime result;
		try
		{
			text = Conversions.ToString(S[33]) + Conversions.ToString(S[34]) + "." + Conversions.ToString(S[30]) + Conversions.ToString(S[31]) + "." + Conversions.ToString(S[25]) + Conversions.ToString(S[26]) + Conversions.ToString(S[27]) + Conversions.ToString(S[28]) + " ";
			text = text + Conversions.ToString(S[36]) + Conversions.ToString(S[37]) + ":" + Conversions.ToString(S[39]) + Conversions.ToString(S[40]) + ":" + Conversions.ToString(S[42]) + Conversions.ToString(S[43]);
			result = Conversions.ToDate(text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.Lg.SaveTextToLog("StrToDt", "Внимание! Ошибка преобразования ответа сервера налоговой в дату.", S + " >>> " + text);
			result = DateTime.Now;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string TimeZ()
	{
		string text = All.MyDoc() + "\\WebCheck\\Temp\\All\\RequestTime.txt";
		if (!FileForSend(text))
		{
			return "";
		}
		return SendFile(text);
	}

	private string SendFile(string FillePath)
	{
		string address = "http://fs.tax.gov.ua:8609/fs/cmd";
		string result;
		try
		{
			using WebClient webClient = new WebClient();
			webClient.Headers.Add("Content-Type", "application/octet-stream");
			Array array = File.ReadAllBytes(FillePath);
			Array array2 = webClient.UploadData(address, "POST", (byte[])array);
			result = Encoding.UTF8.GetString((byte[])array2);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool FileForSend(string fileN)
	{
		if (File.Exists(fileN))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(fileN);
		}
		bool result;
		try
		{
			StreamWriter streamWriter = new StreamWriter(fileN);
			streamWriter.Write("{\"Command\":\"ServerState\"}");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}
}
