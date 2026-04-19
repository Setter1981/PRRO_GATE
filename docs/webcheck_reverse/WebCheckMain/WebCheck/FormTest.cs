using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
internal class FormTest : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("StartTest")]
	private Button _StartTest;

	[CompilerGenerated]
	[AccessedThroughProperty("setting")]
	private Button _setting;

	[CompilerGenerated]
	[AccessedThroughProperty("osp")]
	private Button _osp;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyURLb")]
	private Button _CopyURLb;

	[CompilerGenerated]
	[AccessedThroughProperty("OpenURLb")]
	private Button _OpenURLb;

	[CompilerGenerated]
	[AccessedThroughProperty("SetGetB")]
	private Button _SetGetB;

	private string innOp;

	private IniHGB ospus;

	[field: AccessedThroughProperty("FileInfo")]
	internal virtual TextBox FileInfo
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button StartTest
	{
		[CompilerGenerated]
		get
		{
			return _StartTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = StartTest_Click;
			Button startTest = _StartTest;
			if (startTest != null)
			{
				((Control)startTest).Click -= eventHandler;
			}
			_StartTest = value;
			startTest = _StartTest;
			if (startTest != null)
			{
				((Control)startTest).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TestResult")]
	internal virtual TextBox TestResult
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button setting
	{
		[CompilerGenerated]
		get
		{
			return _setting;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = setting_Click;
			Button val = _setting;
			if (val != null)
			{
				((Control)val).Click -= eventHandler;
			}
			_setting = value;
			val = _setting;
			if (val != null)
			{
				((Control)val).Click += eventHandler;
			}
		}
	}

	internal virtual Button osp
	{
		[CompilerGenerated]
		get
		{
			return _osp;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = osp_Click;
			Button val = _osp;
			if (val != null)
			{
				((Control)val).Click -= eventHandler;
			}
			_osp = value;
			val = _osp;
			if (val != null)
			{
				((Control)val).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("UrlT")]
	internal virtual TextBox UrlT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyURLb
	{
		[CompilerGenerated]
		get
		{
			return _CopyURLb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CopyURLb_Click;
			Button copyURLb = _CopyURLb;
			if (copyURLb != null)
			{
				((Control)copyURLb).Click -= eventHandler;
			}
			_CopyURLb = value;
			copyURLb = _CopyURLb;
			if (copyURLb != null)
			{
				((Control)copyURLb).Click += eventHandler;
			}
		}
	}

	internal virtual Button OpenURLb
	{
		[CompilerGenerated]
		get
		{
			return _OpenURLb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OpenURLb_Click;
			Button openURLb = _OpenURLb;
			if (openURLb != null)
			{
				((Control)openURLb).Click -= eventHandler;
			}
			_OpenURLb = value;
			openURLb = _OpenURLb;
			if (openURLb != null)
			{
				((Control)openURLb).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("NameL")]
	internal virtual Label NameL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SetGetB
	{
		[CompilerGenerated]
		get
		{
			return _SetGetB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SetGetB_Click;
			Button setGetB = _SetGetB;
			if (setGetB != null)
			{
				((Control)setGetB).Click -= eventHandler;
			}
			_SetGetB = value;
			setGetB = _SetGetB;
			if (setGetB != null)
			{
				((Control)setGetB).Click += eventHandler;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_009b: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a5: Expected O, but got Unknown
		//IL_012a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0134: Expected O, but got Unknown
		//IL_01b1: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bb: Expected O, but got Unknown
		//IL_0243: Unknown result type (might be due to invalid IL or missing references)
		//IL_024d: Expected O, but got Unknown
		//IL_02ca: Unknown result type (might be due to invalid IL or missing references)
		//IL_02d4: Expected O, but got Unknown
		//IL_0354: Unknown result type (might be due to invalid IL or missing references)
		//IL_035e: Expected O, but got Unknown
		//IL_03d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_03e1: Expected O, but got Unknown
		//IL_045e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0468: Expected O, but got Unknown
		//IL_04f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_04fe: Expected O, but got Unknown
		//IL_056f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0579: Expected O, but got Unknown
		//IL_06ce: Unknown result type (might be due to invalid IL or missing references)
		//IL_06d8: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTest));
		FileInfo = new TextBox();
		StartTest = new Button();
		TestResult = new TextBox();
		setting = new Button();
		osp = new Button();
		UrlT = new TextBox();
		CopyURLb = new Button();
		OpenURLb = new Button();
		NameL = new Label();
		SetGetB = new Button();
		((Control)this).SuspendLayout();
		((Control)FileInfo).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FileInfo).Location = new Point(12, 12);
		FileInfo.Multiline = true;
		((Control)FileInfo).Name = "FileInfo";
		FileInfo.ScrollBars = (ScrollBars)2;
		((Control)FileInfo).Size = new Size(675, 134);
		((Control)FileInfo).TabIndex = 0;
		((Control)FileInfo).TabStop = false;
		((Control)StartTest).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)StartTest).Location = new Point(12, 152);
		((Control)StartTest).Name = "StartTest";
		((Control)StartTest).Size = new Size(620, 41);
		((Control)StartTest).TabIndex = 0;
		((ButtonBase)StartTest).Text = "Перевірити";
		((ButtonBase)StartTest).UseVisualStyleBackColor = true;
		((Control)TestResult).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TestResult).Location = new Point(12, 199);
		TestResult.Multiline = true;
		((Control)TestResult).Name = "TestResult";
		TestResult.ScrollBars = (ScrollBars)2;
		((Control)TestResult).Size = new Size(675, 211);
		((Control)TestResult).TabIndex = 2;
		((Control)TestResult).TabStop = false;
		((Control)setting).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)setting).Location = new Point(12, 513);
		((Control)setting).Name = "setting";
		((Control)setting).Size = new Size(330, 37);
		((Control)setting).TabIndex = 3;
		((ButtonBase)setting).Text = "settings.ini";
		((ButtonBase)setting).UseVisualStyleBackColor = true;
		((Control)osp).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)osp).Location = new Point(357, 513);
		((Control)osp).Name = "osp";
		((Control)osp).Size = new Size(330, 37);
		((Control)osp).TabIndex = 4;
		((ButtonBase)osp).Text = "ospus.ini";
		((ButtonBase)osp).UseVisualStyleBackColor = true;
		((Control)UrlT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)UrlT).Location = new Point(12, 434);
		((Control)UrlT).Name = "UrlT";
		((TextBoxBase)UrlT).ReadOnly = true;
		((Control)UrlT).Size = new Size(675, 30);
		((Control)UrlT).TabIndex = 5;
		((Control)UrlT).TabStop = false;
		((Control)CopyURLb).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CopyURLb).Location = new Point(12, 470);
		((Control)CopyURLb).Name = "CopyURLb";
		((Control)CopyURLb).Size = new Size(330, 37);
		((Control)CopyURLb).TabIndex = 6;
		((ButtonBase)CopyURLb).Text = "Скопіювати  URL";
		((ButtonBase)CopyURLb).UseVisualStyleBackColor = true;
		((Control)OpenURLb).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OpenURLb).Location = new Point(357, 470);
		((Control)OpenURLb).Name = "OpenURLb";
		((Control)OpenURLb).Size = new Size(330, 37);
		((Control)OpenURLb).TabIndex = 7;
		((ButtonBase)OpenURLb).Text = "Відкрити  URL";
		((ButtonBase)OpenURLb).UseVisualStyleBackColor = true;
		NameL.AutoSize = true;
		((Control)NameL).Font = new Font("Microsoft Sans Serif", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NameL).Location = new Point(12, 413);
		((Control)NameL).Name = "NameL";
		((Control)NameL).Size = new Size(142, 18);
		((Control)NameL).TabIndex = 8;
		NameL.Text = "Рішення проблеми ";
		((Control)SetGetB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SetGetB).Location = new Point(638, 152);
		((Control)SetGetB).Name = "SetGetB";
		((Control)SetGetB).Size = new Size(52, 41);
		((Control)SetGetB).TabIndex = 9;
		((ButtonBase)SetGetB).Text = "...";
		((ButtonBase)SetGetB).UseVisualStyleBackColor = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(702, 561);
		((Control)this).Controls.Add((Control)(object)SetGetB);
		((Control)this).Controls.Add((Control)(object)NameL);
		((Control)this).Controls.Add((Control)(object)OpenURLb);
		((Control)this).Controls.Add((Control)(object)CopyURLb);
		((Control)this).Controls.Add((Control)(object)UrlT);
		((Control)this).Controls.Add((Control)(object)osp);
		((Control)this).Controls.Add((Control)(object)setting);
		((Control)this).Controls.Add((Control)(object)TestResult);
		((Control)this).Controls.Add((Control)(object)StartTest);
		((Control)this).Controls.Add((Control)(object)FileInfo);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTest";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Перевірка налаштувань ПРРО";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormTest(string innOperator)
	{
		((Form)this).Load += FormTest_Load;
		ospus = new IniHGB(All.MyDoc() + "\\WebCheck\\ospus.ini");
		InitializeComponent();
		innOp = innOperator;
	}

	private void FormTest_Load(object sender, EventArgs e)
	{
		FileInfo.Text = GetTextForOutput(All.A.FileN);
		NameL.Text = "Відповіді на типові запитання:";
		UrlT.Text = "https://www.webchek.com.ua/helpie_faq/";
	}

	private string GetTextForOutput(string filePath)
	{
		if (!((ServerComputer)MyProject.Computer).FileSystem.FileExists(filePath))
		{
			throw new Exception("File Not Found: " + filePath);
		}
		StringBuilder stringBuilder = new StringBuilder();
		FileInfo fileInfo = ((ServerComputer)MyProject.Computer).FileSystem.GetFileInfo(filePath);
		stringBuilder.Append("File: " + fileInfo.FullName);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("CreationTime: " + fileInfo.CreationTime);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Modified: " + fileInfo.LastWriteTime);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Size: " + fileInfo.Length + " bytes");
		stringBuilder.Append("\r\n");
		return stringBuilder.ToString();
	}

	private void StartTest_Click(object sender, EventArgs e)
	{
		((Control)StartTest).Enabled = false;
		string section = "\\SOFTWARE\\Institute of Informational Technologies\\Certificate Authority-1.3\\End User\\Libraries\\Sign\\KeyMedia";
		string @string = ospus.GetString(section, "ShowErrors", "0");
		int num = (Versioned.IsNumeric((object)@string) ? Conversions.ToInteger(@string) : 0);
		int retriesPrt = All.RetriesPrt;
		All.RetriesPrt = 3;
		All.SF.ErrorShow(ShowWindows: true);
		StringBuilder stringBuilder = new StringBuilder();
		stringBuilder.Append("WebCheck");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("Version: " + All.VersionDll());
		stringBuilder.Append("\r\n");
		stringBuilder.Append("TimeStart:" + DateTime.Now);
		stringBuilder.Append("\r\n");
		All.A.OperatorINN = innOp;
		TypErrStr typErrStr = new CheckLastCheck().NumberLastCheckTest(innOp);
		All.A.OperatorINN = "";
		stringBuilder.Append("TimeEnd: " + DateTime.Now);
		stringBuilder.Append("\r\n");
		if (typErrStr.errCode > 0)
		{
			stringBuilder.Append("ErrCode: " + typErrStr.errCode);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("ErrString: " + typErrStr.errStr.ToString());
			stringBuilder.Append("\r\n");
			HelpL(typErrStr.errCode);
		}
		else
		{
			stringBuilder.Append("ErrCode: 0");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("ErrString: Not");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("Status: Ок!");
			stringBuilder.Append("\r\n");
			stringBuilder.Append("Last check number on the tax website:" + typErrStr.ReturnStr);
			stringBuilder.Append("\r\n");
		}
		TestResult.Text = stringBuilder.ToString();
		All.RetriesPrt = retriesPrt;
		if (num > 0)
		{
			All.SF.ErrorShow(ShowWindows: true);
		}
		else
		{
			All.SF.ErrorShow(ShowWindows: false);
		}
		((Control)StartTest).Enabled = true;
	}

	private void HelpL(int errN)
	{
		NameL.Text = "Рішення помилки №" + errN + ":";
		int num = errN;
		if (num == -14)
		{
			UrlT.Text = "https://www.webchek.com.ua/helpie_faq/status-14-msg-ne-zareiestrovano-pidpisant/";
			return;
		}
		NameL.Text = "Відповіді на типові запитання:";
		UrlT.Text = "https://www.webchek.com.ua/helpie_faq/";
	}

	private void CopyURLb_Click(object sender, EventArgs e)
	{
		try
		{
			Clipboard.SetText(UrlT.Text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void OpenURLb_Click(object sender, EventArgs e)
	{
		try
		{
			Process.Start(UrlT.Text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void setting_Click(object sender, EventArgs e)
	{
		//IL_002a: Unknown result type (might be due to invalid IL or missing references)
		try
		{
			Process.Start(All.f.FileName, "Notepad.exe");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox((object)"Такого файлу немає!", (MsgBoxStyle)64, (object)"setting.ini");
			ProjectData.ClearProjectError();
		}
	}

	private void osp_Click(object sender, EventArgs e)
	{
		//IL_002b: Unknown result type (might be due to invalid IL or missing references)
		try
		{
			Process.Start(ospus.FileName, "Notepad.exe");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox((object)"Такого файлу немає!", (MsgBoxStyle)64, (object)"setting.ini");
			ProjectData.ClearProjectError();
		}
	}

	private void SetGetB_Click(object sender, EventArgs e)
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		FormHelp formHelp = new FormHelp();
		((Form)formHelp).ShowDialog();
		((Component)(object)formHelp).Dispose();
	}
}
